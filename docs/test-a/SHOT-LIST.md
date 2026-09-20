# The pictures to take at the dry run

For the person at the dry run's machine. Press **Windows + Shift + S**, drag
around the window or dialog only (not the whole screen), and the picture is
in *Pictures → Screenshots*. Take each one **at the moment it appears**, in
this order: it is the order a tester meets them. Nothing personal in frame:
no other windows, no account name where it can be avoided (the two places
where it cannot are marked).

| # | What must be on screen | File name | Where it goes in the sheet |
|---|---|---|---|
| 4 | The page the link opens, **before** anything is clicked: the file's name and the download button. If Drive shows a page saying it cannot scan the file for viruses, or that the file is a program, that page too (4b), with its *Download anyway* button visible. **If no such page appears, say so**: the sheet's conditional sentence is then deleted, not left as a maybe. | `4-download-page.png`, `4b-drive-warning.png` | Step 1, under "The link opens a page…" |
| 5 | The browser's own warning about the download, with the file's name readable: in Edge the downloads list with the "isn't commonly downloaded" line, and (5b) the same list after the three dots were clicked, with **Keep** visible; then (5c) the box with **Show more** opened and **Keep anyway** visible. Whichever browser the machine has: say which. | `5-browser-warning.png`, `5b-keep.png`, `5c-keep-anyway.png` | Step 1, under the Edge and Chrome bullets |
| 6 | The blue **"Windows protected your PC"** window as it first appears: only **More info** and **Don't run** to be seen. | `6-smartscreen.png` | Step 2, first bullet |
| 7 | The same window after **More info** was clicked: the app's name, "Unknown publisher", and the **Run anyway** button visible. | `7-smartscreen-run-anyway.png` | Step 2, first bullet, beside 6 |
| 8 | The installer's pages, every one, in order: (8a) the first page, (8b) the page that shows the folder, (8c) the bar while it installs, (8d) *Completed*, (8e) the last page with *Run localSpace* ticked. **Write down the button that was pressed on each**: the sheet says *Next*, *Next*, *Next*, *Finish*, and only the second has been seen. 8b shows the account's folder name: crop the picture to the top half of the window, above the folder line, or tell the builder to. | `8a-…` to `8e-…` | Step 3 |
| 9 | File Explorer open at the folder that **Windows + R**, `%LOCALAPPDATA%\localSpace\logs` opens, with **app.log** visible in the list. Make the window narrow before the picture, so that the address bar shortens to "… › localSpace › logs" and the account's name is not in it. | `9-the-log-folder.png` | Step 7, under the first bullet |

Also wanted, if it happens: whatever Windows shows **instead** of 6 and 7
(Smart App Control refusing the file, an antivirus asking), exactly as it
appeared.

Pictures 1 to 3 (the first run, the list of models, a chat) are the
builder's, from the release build, and are not taken here.
