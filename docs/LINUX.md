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

1. **Software rendering.** In **Settings → Advanced**, turn on **WebKitGTK
   software rendering**. This uses CPU painting and Mesa compositing over shared
   memory (SHM).
   Disabling compositing entirely can accumulate translucent shadows and old
   animation frames on WebKitGTK 2.50.4. On NVIDIA, Rayburst selects the Mesa EGL
   vendor automatically; if Mesa is missing, it retains the legacy fallback.
   If the window cannot be used, start Rayburst from a terminal (adjust the Mesa
   vendor path if your distribution installs it elsewhere):

   ```bash
   LIBGL_ALWAYS_SOFTWARE=1 \
   __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
   WEBKIT_SKIA_ENABLE_CPU_RENDERING=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 rayburst
   ```

   For the AppImage, put the variables in front of `./Rayburst_x.x.x_amd64.AppImage`.
   Variables set by the launching environment always take precedence over the
   setting. Do not combine SHM compositing with
   `WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`, or a
   forced Mesa `__GLX_VENDOR_LIBRARY_NAME`; these can prevent transparent frames
   from being replaced correctly.

2. **X11 instead of Wayland.** Some Wayland compositors (Hyprland in
   particular) fail to map the window. Start with `GDK_BACKEND=x11`.

3. **NVIDIA.** Rayburst automatically uses the Mesa SHM compositor when Mesa
   EGL is installed. This bypasses NVIDIA DMA-BUF imports without disabling
   compositing. The display server retains its own GLX vendor: forcing Mesa GLX
   on an NVIDIA X server can prevent GTK from creating a transparent GL context.
   Without Mesa EGL, Rayburst keeps the previous DMA-BUF-disabled fallback,
   which can retain transparent frames on WebKitGTK 2.50.4. On Wayland it also
   sets `__NV_DISABLE_EXPLICIT_SYNC=1` to avoid a known WebKitGTK crash.

4. **A distribution package.** The `.deb` and `.rpm` packages use the
   distribution's WebKitGTK and graphics stack. If the AppImage misbehaves on a
   distribution these packages support, prefer them.

When reporting a display problem, include the WebKitGTK version
(`pkg-config --modversion webkit2gtk-4.1`, or your package manager's version
of `webkit2gtk4.1`), the GPU and driver, the session type
(`echo $XDG_SESSION_TYPE`) and `~/.local/share/dev.aninsomniacy.rayburst/logs/rayburst.log`.

The native regression probe in `scripts/diagnostics/webkit-transparent-repaint.c`
checks shadow alpha and empty hidden frames across 20 show/hide cycles. Its header
contains the build command; run it with the environment above, optionally under
`xvfb-run -a` for an isolated display. This tests WebKit's native backing store,
independently of Rayburst's UI and download services.
