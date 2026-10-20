# Linux display problems

Rayburst draws its window with the system's WebKitGTK. Blank or partly drawn
windows, a window that never appears, stuttering animations, and
`WebKitWebProcess` crashes (SIGABRT, heap corruption, `libEGL`/`libGLESv2` in
the backtrace) come from WebKitGTK's GPU path on particular driver, compositor
and WebKitGTK version combinations. Downloads keep working while the window
misbehaves.

WebKitGTK 2.54 repaints only the changed parts of a frame, and with DMA-BUF
imports disabled (NVIDIA, software rendering) windows then showed only the area
around the pointer. Rayburst has WebKitGTK repaint every frame in full, so
these settings no longer cause that.

Try these in order, restarting Rayburst after each:

1. **Software rendering.** In **Settings → Advanced**, turn off **WebKitGTK
   hardware compositing** (the default). Rayburst uses CPU painting and Mesa
   compositing over shared memory (SHM), keeping the compositor active so
   translucent shadows and old animation frames are replaced on WebKitGTK 2.50.4.
   The setting takes effect after restarting. If the window cannot be used,
   set `preferences.hardwareRendering` to `false` in
   `~/.local/share/dev.aninsomniacy.rayburst/config.json` while Rayburst is closed.

   The equivalent environment for the native regression probe is (adjust the Mesa
   vendor path if your distribution installs it elsewhere):

   ```bash
   LIBGL_ALWAYS_SOFTWARE=1 \
   __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
   WEBKIT_SKIA_ENABLE_CPU_RENDERING=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 \
   xvfb-run -a /tmp/rayburst-webkit-repaint
   ```

   Rayburst applies the preference on every start, clearing the previous rendering
   mode's environment variables on relaunch. Do not combine SHM compositing with
   `WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`, or a
   forced Mesa `__GLX_VENDOR_LIBRARY_NAME`; these can prevent transparent frames
   from being replaced correctly.

2. **X11 instead of Wayland.** Some Wayland compositors (Hyprland in
   particular) fail to map the window. Start with `GDK_BACKEND=x11`.

3. **NVIDIA.** Some distribution builds of WebKitGTK disable NVIDIA compositing
   even when the application requests it. With **WebKitGTK hardware compositing**
   on, Rayburst explicitly enables the compositor and uses SHM frame transport
   to avoid GBM buffer allocation and DMA-BUF import failures. This keeps the
   system's graphics vendor and does not force CPU painting. The equivalent
   native probe command, with the software rendering variables unset, is:

   ```bash
   WEBKIT_FORCE_DMABUF_RENDERER=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 \
   /tmp/rayburst-webkit-repaint
   ```

   With hardware compositing off, Rayburst selects the Mesa EGL vendor without
   changing the display server's GLX vendor. Mesa EGL must be installed for this
   software path. Without it, Rayburst retains the DMA-BUF-disabled fallback,
   which can accumulate shadows on WebKitGTK 2.50.4; install your distribution's
   Mesa EGL package and restart Rayburst.

4. **A distribution package.** The `.deb` and `.rpm` packages use the
   distribution's WebKitGTK and graphics stack. If the AppImage misbehaves on a
   distribution these packages support, prefer them.

When reporting a display problem, include the WebKitGTK version
(`pkg-config --modversion webkit2gtk-4.1`, or your package manager's version
of `webkit2gtk4.1`), the GPU and driver, the session type
(`echo $XDG_SESSION_TYPE`) and `~/.local/share/dev.aninsomniacy.rayburst/logs/rayburst.log`.
The diagnostic export records the hardware rendering request separately from the
main WebView's effective acceleration policy. `Never` means WebKit rejected
compositing; `Always` permits it but does not prove that painting uses the GPU.

The native regression probe in `scripts/diagnostics/webkit-transparent-repaint.c`
checks shadow alpha and empty hidden frames across 20 show/hide cycles, and
prints the requested and effective acceleration policies. Its header contains
the build command; run it with the environment above, optionally under
`xvfb-run -a` for an isolated display. `--policy=default` leaves WebKit's policy
unchanged; `--policy=never` provides a non-composited control. This tests WebKit's
native backing store, independently of Rayburst's UI and download services.
