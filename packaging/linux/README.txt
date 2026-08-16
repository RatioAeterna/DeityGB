DeityGB for Linux
==================

Run ./DeityGB to open the application. Press Enter on the startup screen,
choose a folder containing .gb or .gbc files, navigate with W/S, the arrow
keys, or Tab, and launch with J or Enter.

The executable contains the emulator icon and DMG/CGB boot assets. Cartridge
saves and RTC sidecars are written next to the selected ROM.

Keep `deitygb-audio` beside `DeityGB`. Linux audio runs in that isolated helper
so a host audio-driver failure cannot terminate the emulator itself.

The host system must provide the normal desktop OpenGL/X11 or Wayland runtime
and ALSA audio libraries. Folder selection is built into DeityGB and does not
require a separate desktop dialog program. To install this folder as a desktop application, copy
DeityGB to a directory on PATH, DeityGB.desktop to
~/.local/share/applications/, and deitygb.png to
~/.local/share/icons/hicolor/512x512/apps/.
