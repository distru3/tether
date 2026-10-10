# Icons

The Tether app icon (plum squircle, "T" with an orange tether to a clock) in
every size Tauri needs. `icon.png` is the 512×512 source; `icon.ico` is also
the installer and uninstaller icon.

To regenerate the set from a new 1024×1024 source:

    npm run tauri -- icon path/to/source-1024x1024.png

After changing the icon, also redraw the installer art:

    python3 ui/src-tauri/installer/art/make_art.py
