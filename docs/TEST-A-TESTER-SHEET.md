# localSpace — tester sheet (Friday 25 September)

One page. Part 1 is five questions to answer **before** the day; part 2 is
the day itself. Nothing here needs a terminal or any technical knowledge.

> **For us, not for testers — delete this block before sending.** Part 1 can
> go out today. Part 2 describes the build of the 25th: step 5 (what the app
> says about the computer, the recommendation, a download that resumes) is
> items 2 to 4 of the build order and is not in the build yet. Screenshots
> of the two Windows messages are added after the dry run of Thursday 24th,
> from the real build on a real machine; the "10 GB" of part 1 is checked
> against the recommended models then too.

## Part 1 — before the day: five things to send us

Open **Task Manager** (press Ctrl+Shift+Esc) and choose **Performance**.

1. **Graphics card.** Click *GPU* (if there are two, the one that is not
   "Intel UHD" or "AMD Radeon Graphics"). Send the name at the top right and
   the number beside *Dedicated GPU memory*, for example
   "NVIDIA GeForce RTX 4060 Laptop GPU, 8.0 GB". No GPU entry at all is a
   fine answer too: say so.
2. **Memory.** Click *Memory* and send the number at the top right, for
   example "16.0 GB".
3. **Free disk space.** Open File Explorer, choose *This PC*, and send the
   free space of drive C:. localSpace and one model need about 10 GB.
4. **Windows.** Press the Windows key, type *About your PC*, press Enter,
   and send the *Edition* and *Version* lines (for example
   "Windows 11 Home, 24H2").
5. **Smart App Control.** Press the Windows key, type *Smart App Control*,
   press Enter. Send the word that is selected: **On**, **Evaluation** or
   **Off**. **Please do not change it.** It cannot be turned back on without
   reinstalling Windows, and we only need to know.

## Part 2 — on the day

1. **Get the file** we send you: `localSpace-…-setup.exe`.
2. **Run it.** Double-click it. It asks for nothing: *Next*, *Install*,
   *Finish*. It installs for you only and does not ask for an administrator.
3. **If Windows stops it,** it shows one of two messages, because this test
   build is not signed yet:
   - **"Windows protected your PC"** (a blue window): click **More info**,
     then **Run anyway**.
   - **"Smart App Control blocked an app that may be unsafe"**: there is no
     button that lets it through, and nothing to fix on your side. Do not
     change any Windows setting. Tell the person running the test: you will
     take part on another machine or watch a neighbour's.
4. **Start localSpace** from the Start menu if it did not open by itself.
   The first start takes a moment.
5. **Read what it says about your computer**, and the model it recommends.
   Accept it. The download is a few gigabytes; it continues by itself if
   the connection drops.
6. **Chat.** Ask it anything. Then try something long, such as "explain how
   a heat pump works, in detail".

### What to write down

- What the app said about your computer (one line), and the model it
  recommended.
- The speed it promised, and whether the answers felt like that.
- How long the first word of an answer took: under two seconds, a few
  seconds, or longer.
- Anything that stopped you, word for word if there was a message.

If someone asks you for "the log": paste
`%LOCALAPPDATA%\io.localspace.app\data\logs` into the address bar of File
Explorer and send the file `app.log`.

### Afterwards

Keep it or remove it: Windows Settings → Apps → localSpace → Uninstall. Tick
*Delete the application data* to remove the downloaded model too.
