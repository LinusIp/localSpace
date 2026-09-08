# When it feels slow

"It feels slow" has three different causes, and the app tells them apart itself.
Set `LOCALSPACE_PERF=1` and read stderr: one line per second while frames are
drawn.

```
[perf   2196262 ms] 7 host frames/s · host cpu avg 6.88 max 10.66 ms · guest ran 7 (avg 4.99 max 8.97 ms) · frame gap min 155 avg 190 max 210 ms
[perf   2196263 ms]   repaints asked for by: egui-0.36.1\src\containers\tooltip.rs:262 ×28 · crates\localspace-client\src\surface.rs:558 ×7
```

- **host frames/s** — how many frames the window drew.
- **host cpu** — what a frame cost on the CPU, egui and the Client's own work.
- **guest ran** — how often the harness surface was actually entered, and what
  it cost. The runner skips it when nothing changed (§16.3).
- **frame gap** — wall-clock time between frames: what the user feels.
- **repaints asked for by** — the file and line of every `request_repaint` that
  led to a frame. An app that repaints while idle names its own author here.

Read them in this order.

## 1. Frames are expensive

`host cpu avg` above 8 ms, or `guest` above 8 ms: the work is the problem.
A slow surface gets a badge and is throttled to twenty repaints a second; the
fix is in the harness (see `HARNESS-AUTHORING.md` §4). A slow host frame is
the Client's: profile `ui()`.

## 2. Frames are cheap but too many

`host frames/s` high while nothing moves, with a cause listed every second:
something asks for repaints in a loop. Idle must mean idle — the Client
repaints only for input, for Core, or for a surface's own request.

## 3. Frames are cheap and too few

`host cpu` of a millisecond or two, yet `frame gap` a uniform 150–200 ms: the
app is producing frames faster than the screen accepts them. Confirm with
`LOCALSPACE_PERF_SPIN=1`, which asks for a repaint every frame with no input
at all; the frame rate it reaches is the ceiling of the display path, and
nothing in the app moves that ceiling.

The variables below change how frames are presented, for ruling things out:

```
WGPU_BACKEND=dx12|vulkan            which GPU API
WGPU_POWER_PREF=low|high            integrated or discrete GPU
LOCALSPACE_PRESENT=vsync|novsync|immediate|mailbox|fifo
LOCALSPACE_FRAME_LATENCY=1|2        swapchain queue depth
```

The `gpu:` line in the log says which adapter, backend and driver were used.

**The worked example, from a Ryzen 7 6800H laptop with an RTX 3050 Ti:** every
combination above gave 6–7 frames a second at 1.5 ms of CPU each, with frame
gaps of 155–200 ms, foreground or not. `dxdiag` showed why: the internal panel
is wired to the AMD integrated GPU (no MUX), that device had driver problem
code 31 (`CM_PROB_FAILED_ADD`), and Windows was driving the panel with the
Microsoft Basic Display Driver. Every application on that machine presented at
that rate. The fix was the AMD graphics driver, not the app.
