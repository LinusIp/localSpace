localSpace
==========

A private AI workstation. The model runs on this computer. localSpace goes
online only to download a model you chose, or when you let it look
something up on the web.

Starting it
-----------
Installed:  open localSpace from the Start menu.
From a zip: open the folder you unpacked and double-click localspace-app.

The first start takes a moment: localSpace sets up its folder, then shows
its window.

If Windows stops it
-------------------
This build is not signed yet, so Windows may warn about it.

"Windows protected your PC": choose "More info", then "Run anyway".

"Smart App Control blocked an app that may be unsafe" has no such button,
and there is no way around it from your side. Please do not change any
Windows setting for this; tell the person running the test.

Where things are
----------------
Your conversations, boards and downloaded models:
    %LOCALAPPDATA%\localSpace
The log, if someone asks you for it:
    %LOCALAPPDATA%\localSpace\logs\app.log

Paste either line into the address bar of File Explorer to go there.

Removing it
-----------
Installed:  Windows Settings > Apps, find localSpace, choose Uninstall.
            Tick "Delete the application data" to remove your conversations
            and the downloaded models as well; leave it empty to keep them.
From a zip: delete the folder you unpacked. Your data stays in the folder
            named above until you delete that too.

What is inside
--------------
localspace-app.exe   the application
localspace.exe       the command-line tool; "localspace doctor" describes
                     this computer for support
engine\              llama.cpp (release b10869, Vulkan), which runs the model
web\, registry\      the interface, and the tools the Store offers
licences\            the licences of what is included
