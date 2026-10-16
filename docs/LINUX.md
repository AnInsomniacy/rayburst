# Linux display problems

Rayburst draws its window with the system's WebKitGTK. Blank or partly drawn
windows, a window that never appears, stuttering animations, and
`WebKitWebProcess` crashes (SIGABRT, heap corruption, `libEGL`/`libGLESv2` in
the backtrace) come from WebKitGTK's GPU path on particular driver, compositor
and WebKitGTK version combinations. Downloads keep working while the window
misbehaves.

WebKitGTK 2.54 repaints only the changed parts of a frame, and with DMA-BUF
rendering off (NVIDIA, software rendering) windows then showed only the area
around the pointer. Rayburst has WebKitGTK repaint every frame in full, so
these settings no longer cause that.

Try these in order, restarting Rayburst after each:

1. **Software rendering.** In **Settings → Advanced**, turn on **WebKitGTK
   software rendering**. If the window cannot be used, start Rayburst once
   with the same switch from a terminal:

   ```bash
   WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 rayburst
   ```

   For the AppImage, put the variables in front of `./Rayburst_x.x.x_amd64.AppImage`.
   Variables set by the launching environment always take precedence over the
   setting.

2. **X11 instead of Wayland.** Some Wayland compositors (Hyprland in
   particular) fail to map the window. Start with `GDK_BACKEND=x11`.

3. **NVIDIA.** Rayburst already sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` with
   the NVIDIA driver, which otherwise leaves the window blank, and on Wayland
   also `__NV_DISABLE_EXPLICIT_SYNC=1`, which avoids a known WebKitGTK crash.
   If you start it with your own values, keep them at `1`.

4. **A distribution package.** The `.deb` and `.rpm` packages use the
   distribution's WebKitGTK and graphics stack. If the AppImage misbehaves on a
   distribution these packages support, prefer them.

When reporting a display problem, include the WebKitGTK version
(`pkg-config --modversion webkit2gtk-4.1`, or your package manager's version
of `webkit2gtk4.1`), the GPU and driver, the session type
(`echo $XDG_SESSION_TYPE`) and `~/.local/share/dev.aninsomniacy.rayburst/logs/rayburst.log`.
