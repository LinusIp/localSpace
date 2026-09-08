//! Tier A — the default. A wasm component under wasmtime with capability-based
//! isolation and no ambient authority.
//!
//! Everything the harness can reach is an explicit import in `wit/harness.wit`,
//! and every one of those imports is checked here against the capabilities the
//! package declared before Core will service it.

use super::{CoreServices, HarnessOutput, HarnessRuntime, RuntimeConfig};
use crate::manifest::{Capabilities, DocsCap, ModelCap, NetCap};
use anyhow::{Context, Result};
use serde_json::Value as J;
use std::sync::Arc;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../wit",
    world: "harness",
});

use localspace::harness::host::Host as HarnessHost;

/// wasmtime 48 carries its own error type, which is not a `std::error::Error`,
/// so `anyhow::Context` does not apply to it. This is the one adapter.
trait WtExt<T> {
    fn wt(self, context: &str) -> Result<T>;
}

impl<T> WtExt<T> for wasmtime::Result<T> {
    fn wt(self, context: &str) -> Result<T> {
        self.map_err(|e| anyhow::anyhow!("{e}"))
            .context(context.to_string())
    }
}

/// Store data for one running harness. Holds the call-scoped document so that
/// `doc-get` / `doc-put` never re-enter Core.
pub struct HostState {
    harness_id: String,
    caps: Capabilities,
    services: Arc<dyn CoreServices>,
    doc_in: J,
    doc_out: Option<J>,
    logs: Vec<String>,
    wasi: WasiCtx,
    table: ResourceTable,
    /// `[resources] memory_mb.logic`, in bytes.
    memory_budget: usize,
    /// Set the moment the guest asks for more than its budget. The grow is
    /// refused; the guest usually traps on the failed allocation; Core reads
    /// this to say why, then kills and restarts the instance.
    over_budget: Option<usize>,
}

/// The enforcement point for `[resources] memory_mb.logic` (spec §1.2).
impl wasmtime::ResourceLimiter for HostState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.memory_budget {
            self.over_budget = Some(desired);
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // Tables hold function references; a million is far past any real
        // harness and small enough that it cannot be a memory attack.
        Ok(desired <= 1_000_000)
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl HostState {
    fn deny(&self, cap: &str) -> String {
        format!(
            "capability `{cap}` is not granted to {} — declare it in harness.toml",
            self.harness_id
        )
    }
}

impl HarnessHost for HostState {
    fn doc_get(&mut self) -> String {
        self.doc_in.to_string()
    }

    fn doc_put(&mut self, json: String) -> Result<(), String> {
        let parsed: J = serde_json::from_str(&json)
            .map_err(|e| format!("doc-put was handed invalid JSON: {e}"))?;
        self.doc_out = Some(parsed);
        Ok(())
    }

    fn log(&mut self, message: String) {
        // Bounded: a chatty harness must not grow Core's memory without limit.
        if self.logs.len() < 256 {
            self.logs.push(message);
        }
    }

    fn now_ms(&mut self) -> u64 {
        crate::dag::now_ms()
    }

    fn model_complete(&mut self, prompt: String) -> Result<String, String> {
        if !self.caps.model.contains(&ModelCap::Complete) {
            return Err(self.deny("model.complete"));
        }
        self.services.model_complete(&prompt)
    }

    fn model_structured(&mut self, schema: String, prompt: String) -> Result<String, String> {
        if !self.caps.model.contains(&ModelCap::Structured) {
            return Err(self.deny("model.structured"));
        }
        self.services.model_structured(&schema, &prompt)
    }

    fn model_embed(&mut self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        if !self.caps.model.contains(&ModelCap::Embed) {
            return Err(self.deny("model.embed"));
        }
        self.services.model_embed(&texts)
    }

    fn docs_search(&mut self, query: String) -> Result<String, String> {
        if self.caps.docs != DocsCap::Acl {
            return Err(self.deny("docs"));
        }
        self.services.docs_search(&query)
    }

    fn net_fetch(&mut self, url: String, mode: String) -> Result<String, String> {
        // Two gates: the package's own allowlist, then the environment's mode,
        // which the gateway applies on the other side of this call.
        let host = url_host(&url).ok_or_else(|| format!("`{url}` is not a fetchable URL"))?;
        let allowed = match &self.caps.net {
            NetCap::Simple(_) => false,
            NetCap::Allowlist { allowlist, .. } => {
                allowlist.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
            }
        };
        if !allowed {
            return Err(format!(
                "`{host}` is not in {}'s net allowlist",
                self.harness_id
            ));
        }
        self.services.net_fetch(&self.harness_id, &url, &mode)
    }
}

fn url_host(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

pub struct WasmHarness {
    store: Store<HostState>,
    bindings: Harness,
}

impl WasmHarness {
    pub fn load(bytes: &[u8], cfg: RuntimeConfig) -> Result<WasmHarness> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        // Fuel bounds a runaway harness rather than hanging the Client.
        config.consume_fuel(true);
        // Compiled code is cached on disk by module hash, in wasmtime's own cache
        // under the user's cache directory. A harness is compiled once per
        // machine, not on every launch — §16.4's install-time AOT, without a
        // separate build step.
        match wasmtime::Cache::from_file(None) {
            Ok(cache) => {
                config.cache(Some(cache));
            }
            Err(e) => tracing::warn!("wasm compile cache unavailable, compiling every launch: {e}"),
        }
        let engine = Engine::new(&config).wt("creating the wasm engine")?;

        let component = Component::from_binary(&engine, bytes)
            .wt("loading the harness logic module (expected a wasm component)")?;

        // No preopened directories, no inherited env, no sockets: a Tier A harness
        // reaches the outside world only through the `host` interface above.
        let wasi = WasiCtxBuilder::new().build();

        let state = HostState {
            harness_id: cfg.harness_id.clone(),
            caps: cfg.capabilities,
            services: cfg.services,
            doc_in: J::Object(Default::default()),
            doc_out: None,
            logs: Vec::new(),
            wasi,
            table: ResourceTable::new(),
            memory_budget: cfg.logic_memory_mb as usize * 1024 * 1024,
            over_budget: None,
        };

        let mut store = Store::new(&engine, state);
        store.set_fuel(FUEL_PER_CALL).wt("setting fuel")?;
        // Every memory.grow — including the initial allocation at instantiation —
        // is checked against the declared budget.
        store.limiter(|s| s);

        let mut linker: Linker<HostState> = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).wt("linking WASI p2")?;
        localspace::harness::host::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |s| s,
        )
        .wt("linking the localspace host interface")?;

        let bindings = match Harness::instantiate(&mut store, &component, &linker) {
            Ok(b) => b,
            Err(e) => {
                if let Some(asked) = store.data().over_budget {
                    anyhow::bail!(
                        "{} needs {} MB of memory just to start, over its declared \
                         [resources] memory_mb.logic = {} MB",
                        store.data().harness_id,
                        asked.div_ceil(1024 * 1024),
                        store.data().memory_budget / (1024 * 1024)
                    );
                }
                return Err(anyhow::anyhow!("{e}")).context("instantiating the harness component");
            }
        };

        Ok(WasmHarness { store, bindings })
    }

    /// Turn a trap that followed a refused memory grow into a message that
    /// names the budget, so the user learns why rather than "unreachable".
    fn budgeted(&self, e: anyhow::Error) -> anyhow::Error {
        match self.store.data().over_budget {
            Some(asked) => anyhow::anyhow!(
                "{} exceeded its declared [resources] memory_mb.logic = {} MB (asked for {} MB) \
                 and was stopped",
                self.store.data().harness_id,
                self.store.data().memory_budget / (1024 * 1024),
                asked.div_ceil(1024 * 1024)
            ),
            None => e,
        }
    }

    /// Refill fuel and install the call-scoped document.
    fn begin(&mut self, doc: &J) -> Result<()> {
        self.store.set_fuel(FUEL_PER_CALL).wt("setting fuel")?;
        let data = self.store.data_mut();
        data.doc_in = doc.clone();
        data.doc_out = None;
        data.logs.clear();
        Ok(())
    }

    fn finish(&mut self) -> (Option<J>, Vec<String>) {
        let data = self.store.data_mut();
        (data.doc_out.take(), std::mem::take(&mut data.logs))
    }
}

/// Enough for a large board edit, small enough that an infinite loop trips in
/// well under a second rather than wedging the Client.
const FUEL_PER_CALL: u64 = 2_000_000_000;

impl HarnessRuntime for WasmHarness {
    fn over_budget(&self) -> Option<u64> {
        self.store.data().over_budget.map(|b| b as u64)
    }

    fn tools_json(&mut self) -> Result<String> {
        self.store.set_fuel(FUEL_PER_CALL).wt("setting fuel")?;
        self.bindings
            .call_tools(&mut self.store)
            .wt("harness `tools` export trapped")
    }

    fn call(&mut self, name: &str, params: &J, doc: &J) -> Result<HarnessOutput> {
        self.begin(doc)?;
        let raw = self
            .bindings
            .call_call(&mut self.store, name, &params.to_string())
            .wt(&format!("harness `call` export trapped on `{name}`"))
            .map_err(|e| self.budgeted(e))?;
        let (doc_out, logs) = self.finish();

        let parsed: J = serde_json::from_str(&raw)
            .with_context(|| format!("harness returned non-JSON from `{name}`: {raw}"))?;
        let mut out = HarnessOutput::from_json(&parsed);
        out.doc = doc_out;
        out.logs = logs;
        Ok(out)
    }

    fn context(&mut self, budget: usize, focused: bool, doc: &J) -> Result<(String, bool)> {
        self.begin(doc)?;
        let raw = self
            .bindings
            .call_context(&mut self.store, budget as u32, focused)
            .wt("harness `context` export trapped")
            .map_err(|e| self.budgeted(e))?;
        let parsed: J = serde_json::from_str(&raw).unwrap_or(J::Null);
        let text = parsed
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or(&raw)
            .to_string();
        let expandable = parsed
            .get("expandable")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        Ok((text, expandable))
    }

    fn view(&mut self, view_id: &str, doc: &J) -> Result<J> {
        self.begin(doc)?;
        let raw = self
            .bindings
            .call_view(&mut self.store, view_id)
            .wt("harness `view` export trapped")?;
        serde_json::from_str(&raw).with_context(|| format!("view `{view_id}` returned non-JSON"))
    }

    fn event(&mut self, view_id: &str, payload: &[u8], doc: &J) -> Result<(Vec<u8>, Option<J>)> {
        self.begin(doc)?;
        let reply = self
            .bindings
            .call_event(&mut self.store, view_id, payload)
            .wt("harness `event` export trapped")
            .map_err(|e| self.budgeted(e))?;
        let (doc_out, _) = self.finish();
        Ok((reply, doc_out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_hosts_are_extracted_for_the_allowlist_check() {
        assert_eq!(url_host("https://tiles.example.com/1/2/3.png").as_deref(), Some("tiles.example.com"));
        assert_eq!(url_host("http://a.b.c:8080/x").as_deref(), Some("a.b.c"));
        assert_eq!(url_host("https://user@Host.EXAMPLE.com/p").as_deref(), Some("host.example.com"));
        assert_eq!(url_host("not a url"), None);
    }
}
