# Linux display problems

Rayburst draws its window with the system's WebKitGTK. Blank or partly drawn
windows, a window that never appears, stuttering animations, and
`WebKitWebProcess` crashes (SIGABRT, heap corruption, `libEGL`/`libGLESv2` in
the backtrace) come from WebKitGTK's GPU path on particular driver, compositor
and WebKitGTK version combinations. Downloads keep working while the window
misbehaves.

Rayburst keeps WebKitGTK's compositor on and hands frames over in shared
memory: without compositing, WebKitGTK 2.50 leaves translucent shadows and old
frames on screen. By default it paints on the CPU with Mesa's software GL, away
from the GPU drivers that cause most of these problems. WebKitGTK 2.54 repaints
only the changed parts of a frame, which left windows partly drawn; Rayburst has
it repaint every frame in full.

Try these in order, restarting Rayburst after each:

1. **GPU acceleration.** In **Settings → Advanced**, **GPU acceleration** draws
   the window with the graphics card. It is off by default; turn it on for
   smoother animations, and back off if windows freeze or stay blank. If the
   window cannot be used, set `preferences.hardwareRendering` to `false` in
   `~/.local/share/dev.aninsomniacy.rayburst/config.json` while Rayburst is closed.

2. **X11 instead of Wayland.** Some Wayland compositors (Hyprland in
   particular) fail to map the window. Start with `GDK_BACKEND=x11`.

3. **A distribution package.** The `.deb` and `.rpm` packages use the
   distribution's WebKitGTK and graphics stack. If the AppImage misbehaves on a
   distribution these packages support, prefer them.

When reporting a display problem, include the WebKitGTK version
(`pkg-config --modversion webkit2gtk-4.1`, or your package manager's version
of `webkit2gtk4.1`), the GPU and driver, the session type
(`echo $XDG_SESSION_TYPE`) and `~/.local/share/dev.aninsomniacy.rayburst/logs/rayburst.log`.

The native regression probe in `scripts/diagnostics/webkit-transparent-repaint.c`
checks shadow alpha and empty hidden frames across 20 show/hide cycles. Its header
contains the build command. Run it with Rayburst's default environment, optionally
under `xvfb-run -a` for an isolated display; leave out the last two variables to
test the GPU path:

```bash
WEBKIT_FORCE_DMABUF_RENDERER=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1 WEBKIT_SKIA_ENABLE_CPU_RENDERING=1 LIBGL_ALWAYS_SOFTWARE=1 /tmp/rayburst-webkit-repaint
```

This tests WebKit's native backing store, independently of Rayburst's UI and
download services.
