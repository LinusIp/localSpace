# Trying localSpace on your own computer

Thank you for doing this. localSpace is an AI assistant that runs **on your
computer**: what you type stays there. We want to know whether it installs
and runs on machines we have never seen, and yours is one of them.

> **Stuck, or unsure whether to go on? Ask, whatever the hour.**
> Telegram: ‹the testers' group, or the contact› — the quickest, and you see
> what the others ran into.
> Email: **locallabs.io@gmail.com** (the localLabs team).

**You are on your own for this, and that is the point.** Nothing here needs a
terminal or any technical knowledge. If something stops you, that is not
your mistake: it is exactly what we need to hear about. Stop, write down
what you saw, and tell us.

- **Time:** about 45 minutes, most of it a download you do not have to watch.
- **You need:** Windows 10 or 11, about 10 GB of free disk space, and an
  internet connection for the download (between 1 and 8 GB, once).

## Before you start: one thing about Windows

**This test build is not signed yet.** Signing is how Windows recognises a
publisher, and we are still waiting for our certificate. So Windows will warn
you about this file, and your browser may too. That is expected for a test
build, and steps 1 and 2 show what the warnings look like and what to click.
You are right to be careful with such warnings in general; this once, for
this file, from us, it is safe to go on.

## The steps

**1. Download the installer** from the link in our email:
`localSpace-…-windows-x64-setup.exe` (about 35 MB).

The link opens a page of Google Drive. If Drive says that it cannot scan
the file for viruses, or warns that the file is a program, click
**Download anyway**.

‹picture: Drive's page, from the dry run›

Then your browser may hold the file back, because few people have
downloaded it yet:

- **Edge** says *"… isn't commonly downloaded. Make sure you trust … before
  you open it."* Point at the file in the downloads list, click the three
  dots **…**, choose **Keep**, then **Show more**, then **Keep anyway**.
- **Chrome** says *"… isn't commonly downloaded and may be dangerous."* Click
  the arrow beside it and choose **Keep**.

‹picture: the browser's warning, from the dry run›

**2. Run it.** Double-click the file.

- A blue window may say **"Windows protected your PC"**. Click **More info**
  (small, under the text), then the button **Run anyway** that appears.

  ‹picture: the blue window before and after "More info", from the dry run›

- If instead a message says **"Smart App Control blocked an app that may be
  unsafe"**, there is no button that lets it through, and nothing for you to
  fix. **Do not change any Windows setting.** Stop here and tell us: that
  alone is a useful result.

**3. Install.** Click **Next**, and **Next** again on the page that shows the
folder (leave the folder as it is). When the bar is full and it says
*Completed*, click **Next**, then **Finish**, and leave *Run localSpace*
ticked. It installs for you only, asks for no administrator password, and
takes under a minute.

‹pictures: the installer's pages in order, from the dry run›

**4. The first start.** localSpace opens by itself (if not: Start menu →
*localSpace*). For a few seconds it says *"Looking at this computer…"*. Then
it shows what it found and the model it recommends for your computer, with
how fast it expects it to be:

![The first start: what the computer is, the recommended model, and a green button](test-a/first-run-download.png)

Yours will name your own graphics card and may recommend a different model.
**Please write down, or photograph, what this page says.**

(On a few Windows 10 computers a small window appears instead, saying that
localSpace needs a part of Windows called *Microsoft Edge WebView2*. Click
**Open Microsoft's page**, find *Evergreen Bootstrapper* there, choose
*Download*, run the file it gives you, and start localSpace again. Tell us
that it happened.)

**5. Click the green button, *Download and start*.** This is the long step.

| What you see | How long | It is working when |
|---|---|---|
| *"… % downloaded"* under a bar | 5 minutes to an hour, by your connection: the model is between 1 and 8 GB | the percentage goes up. If your connection drops, it carries on by itself. You can stop it (**Stop the download**, under the button) and go on later with **Continue the download**; closing the app does the same. Nothing that came is lost. |
| *"Checking that the file on this computer is the published one…"* | up to a minute | it says so |
| *"Starting it up. A larger model takes a minute."* | 10 seconds to 2 minutes | it says so |

You do not have to watch. When it is ready, the page changes to the chat by
itself.

**6. Chat, in English.** Ask it anything you like: a question, an email to
write, something to summarise, a sum. Then try something long, such as
*"Explain how a heat pump works, in detail."* Ten minutes is plenty.

- It answers from what it has learned. It cannot look up today's weather or
  the news, and it can be wrong, the smaller models more often: if an answer
  is plainly wrong, we want to see it (question 7 below).
- **Nothing you type leaves your computer.**
- Other languages are not part of this test. If you try one anyway, tell us
  what happened.

**7. Send us two things**, as a reply to our email:

- **The file `app.log`.** Press the Windows key and **R** together, paste
  `%LOCALAPPDATA%\localSpace\logs` and press Enter. A folder opens: attach
  the file **app.log** from it (and *app.log.1* if there is one). It holds
  what the app found out about your computer, what it downloaded and how
  fast it ran. **It holds nothing you typed and none of the answers.** The
  app never sends it anywhere by itself; only you can.

  ‹picture: the folder with app.log in it, from the dry run›
- **Your answers to the ten questions below.** Short is fine.

## The ten questions

1. Did the app install? If not: what did you see, and at which step did you
   stop?
2. Did your browser or Windows warn you? What did it say, and what did you
   do?
3. What did the app say about your computer? (Copy the sentence under *This
   computer*.)
4. Which model did it recommend, with what speed, and did you accept it?
5. Roughly how long did the download take?
6. Was the speed of the answers about what the app promised: faster, slower,
   or about right?
7. Did any answer come back plainly wrong? Paste one if so, with what you
   asked.
8. Was there any moment when you did not know what to do next, or what the
   app was doing?
9. Would you use this again? Why, or why not?
10. Anything else.

## Afterwards

Keep it, or remove it: Windows **Settings → Apps → Installed apps →
localSpace → Uninstall** (on Windows 10: *Apps & features*). Tick *Delete
the application data* to remove the downloaded model too (it is the large
part). To free the space and keep the app: **Settings → Assistant →
Delete**, beside the model.
