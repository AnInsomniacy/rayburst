/*
 * Native WebKitGTK regression probe for transparent backing-store updates.
 * This uses an isolated page, not Rayburst's desktop UI or IPC.
 *
 * Build:
 *   cc -Wall -Wextra -Werror scripts/diagnostics/webkit-transparent-repaint.c \
 *     -o /tmp/rayburst-webkit-repaint $(pkg-config --cflags --libs webkit2gtk-4.1)
 *
 * Run with the rendering environment under investigation. A working renderer
 * preserves the shadow alpha across 20 show/hide cycles and clears every hidden
 * frame. Exit status: 0 = pass, 1 = pixel regression, 2 = diagnostic failure.
 */
#include <gtk/gtk.h>
#include <stdint.h>
#include <webkit2/webkit2.h>

static GtkWidget *window;
static WebKitWebView *view;
static unsigned step;
static unsigned first_alpha;
static int result;

static void fail(const char *message)
{
    g_printerr("Diagnostic failed: %s\n", message);
    result = 2;
    gtk_main_quit();
}

static gboolean clear_parent(GtkWidget *widget, cairo_t *cr, gpointer data)
{
    (void)widget;
    (void)data;
    /* Match Tao's transparent parent; the WebView owns a child GDK surface. */
    cairo_save(cr);
    cairo_set_operator(cr, CAIRO_OPERATOR_SOURCE);
    cairo_set_source_rgba(cr, 0, 0, 0, 0);
    cairo_paint(cr);
    cairo_restore(cr);
    return FALSE;
}

static gboolean sample(unsigned *shadow, unsigned *panel)
{
    cairo_surface_t *surface = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, 128, 128);
    cairo_t *cr = cairo_create(surface);
    gtk_widget_draw(GTK_WIDGET(view), cr);
    cairo_surface_flush(surface);
    cairo_status_t status = cairo_status(cr);
    if (status == CAIRO_STATUS_SUCCESS)
        status = cairo_surface_status(surface);
    if (status == CAIRO_STATUS_SUCCESS) {
        const unsigned char *pixels = cairo_image_surface_get_data(surface);
        int stride = cairo_image_surface_get_stride(surface);
        *shadow = (*(const uint32_t *)(pixels + 64 * stride + 26 * 4)) >> 24;
        *panel = (*(const uint32_t *)(pixels + 64 * stride + 64 * 4)) >> 24;
    }
    cairo_destroy(cr);
    cairo_surface_destroy(surface);
    if (status != CAIRO_STATUS_SUCCESS) {
        fail(cairo_status_to_string(status));
        return FALSE;
    }
    return TRUE;
}

static void evaluated(GObject *source, GAsyncResult *async_result, gpointer data)
{
    (void)data;
    GError *error = NULL;
    JSCValue *value = webkit_web_view_evaluate_javascript_finish(
        WEBKIT_WEB_VIEW(source), async_result, &error);
    if (!value) {
        fail(error ? error->message : "JavaScript evaluation returned no result");
        g_clear_error(&error);
        return;
    }
    g_object_unref(value);
}

static gboolean advance(gpointer data)
{
    (void)data;
    unsigned shadow = 0;
    unsigned panel = 0;
    if (!sample(&shadow, &panel))
        return G_SOURCE_REMOVE;
    gboolean visible = step % 2 == 0;
    if (visible) {
        if (step == 0) {
            if (!shadow || panel != 255) {
                fail("The initial frame did not contain the expected shadow and opaque panel");
                return G_SOURCE_REMOVE;
            }
            first_alpha = shadow;
        }
        g_print("cycle=%u shadow_alpha=%u baseline=%u\n", step / 2, shadow, first_alpha);
        if (ABS((int)shadow - (int)first_alpha) > 1 || panel != 255)
            result = 1;
        if (step == 40) {
            g_print("%s: 20 native show/hide cycles\n", result ? "FAIL" : "PASS");
            gtk_main_quit();
            return G_SOURCE_REMOVE;
        }
    } else {
        if (shadow || panel) {
            g_printerr("Stale hidden frame: shadow_alpha=%u panel_alpha=%u\n", shadow, panel);
            result = 1;
        }
        gtk_widget_hide(window);
        gtk_widget_show_all(window);
    }
    webkit_web_view_evaluate_javascript(view,
        visible ? "document.querySelector('div').style.opacity='0'"
                : "document.querySelector('div').style.opacity='1'",
        -1, NULL, NULL, NULL, evaluated, NULL);
    step++;
    return G_SOURCE_CONTINUE;
}

static void loaded(WebKitWebView *web_view, WebKitLoadEvent event, gpointer data)
{
    (void)web_view;
    (void)data;
    if (event == WEBKIT_LOAD_FINISHED)
        g_timeout_add(300, advance, NULL);
}

static gboolean load_failed(WebKitWebView *web_view, WebKitLoadEvent event,
    const char *uri, GError *error, gpointer data)
{
    (void)web_view;
    (void)event;
    (void)uri;
    (void)data;
    fail(error->message);
    return TRUE;
}

static gboolean timed_out(gpointer data)
{
    (void)data;
    fail("Timed out waiting for native repaint cycles");
    return G_SOURCE_REMOVE;
}

int main(int argc, char **argv)
{
    if (!gtk_init_check(&argc, &argv)) {
        g_printerr("Diagnostic failed: GTK could not open the display\n");
        return 2;
    }
    window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_default_size(GTK_WINDOW(window), 128, 128);
    gtk_window_set_decorated(GTK_WINDOW(window), FALSE);
    GdkVisual *visual = gdk_screen_get_rgba_visual(gtk_widget_get_screen(window));
    if (!visual) {
        g_printerr("Diagnostic failed: The display has no RGBA visual\n");
        return 2;
    }
    gtk_widget_set_visual(window, visual);
    gtk_widget_set_app_paintable(window, TRUE);
    g_signal_connect(window, "draw", G_CALLBACK(clear_parent), NULL);
    view = WEBKIT_WEB_VIEW(webkit_web_view_new());
    GdkRGBA transparent = {0, 0, 0, 0};
    webkit_web_view_set_background_color(view, &transparent);
    webkit_settings_set_hardware_acceleration_policy(
        webkit_web_view_get_settings(view), WEBKIT_HARDWARE_ACCELERATION_POLICY_ALWAYS);
    g_signal_connect(view, "load-changed", G_CALLBACK(loaded), NULL);
    g_signal_connect(view, "load-failed", G_CALLBACK(load_failed), NULL);
    gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));
    webkit_web_view_load_html(view,
        "<!doctype html><style>html,body{margin:0;background:transparent;overflow:hidden}"
        "div{position:absolute;left:32px;top:32px;width:64px;height:64px;background:white;"
        "box-shadow:0 0 12px rgba(0,0,0,.3);transition:opacity 170ms}</style><div></div>", NULL);
    gtk_widget_show_all(window);
    g_print("WebKitGTK %u.%u.%u\n", webkit_get_major_version(),
        webkit_get_minor_version(), webkit_get_micro_version());
    g_timeout_add_seconds(30, timed_out, NULL);
    gtk_main();
    gtk_widget_destroy(window);
    return result;
}
