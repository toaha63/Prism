// gui_gtk.c - GTK wrapper for Prism interpreter with UTF-8 support
#include <gtk/gtk.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <locale.h>
#include <glib.h>
#include <stdio.h>
#include "gui_gtk.h"

// Debug flag - set to 1 to see debug output
#define DEBUG 0

static int gtk_initialized = 0;
static int callback_debug = DEBUG;

void gui_initialize(void)
{
    if (!gtk_initialized)
    {
        setlocale(LC_ALL, "");
        
        int argc = 1;
        char* argv[] = {"prism", NULL};
        char** argv_ptr = argv;
        
        gtk_init(&argc, &argv_ptr);
        gtk_initialized = 1;
    }
}

typedef struct
{
    GtkWidget* window;
    GtkWidget* fixed;
} GUIFrame;

// Global callback for Rust
void (*global_callback)(const char*);

void gui_set_callback(void (*cb)(const char*))
{
    global_callback = cb;
    if (callback_debug) {
        printf("DEBUG: gui_set_callback called, callback=%p\n", (void*)cb);
    }
}

// Window destroy handler - called when user clicks X button
void on_window_destroy(GtkWidget* widget, gpointer data)
{
    exit(0);
}

// Button click handler - passes callback string to Rust
void on_button_click(GtkWidget* widget, gpointer data)
{
    const char* callback_str = (const char*)data;
    if (callback_debug) {
        printf("DEBUG: Button clicked, callback: %s\n", callback_str);
    }
    
    if (global_callback)
    {
        global_callback(callback_str);
    } else {
        if (callback_debug) {
            printf("DEBUG: global_callback is NULL!\n");
        }
    }
}

// Convert string to UTF-8 safely
gchar* to_utf8(const char* text)
{
    if (!text) return g_strdup("");
    
    gchar* utf8 = g_locale_to_utf8(text, -1, NULL, NULL, NULL);
    if (!utf8) {
        utf8 = g_strdup(text);
    }
    return utf8;
}

void* gui_frame_new(const char* title, int width, int height, int x, int y)
{
    gui_initialize();
    
    gchar* utf8_title = to_utf8(title);
    
    GtkWidget* window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), utf8_title);
    gtk_window_set_default_size(GTK_WINDOW(window), width, height);
    gtk_window_set_position(GTK_WINDOW(window), GTK_WIN_POS_CENTER);
    
    g_free(utf8_title);
    
    g_signal_connect(window, "destroy", G_CALLBACK(on_window_destroy), NULL);
    
    GtkWidget* fixed = gtk_fixed_new();
    gtk_container_add(GTK_CONTAINER(window), fixed);
    
    GUIFrame* frame = malloc(sizeof(GUIFrame));
    if (!frame) {
        fprintf(stderr, "ERROR: Failed to allocate memory for GUIFrame\n");
        exit(1);
    }
    frame->window = window;
    frame->fixed = fixed;
    
    return frame;
}

void* gui_label_new(void* frame, const char* text, int x, int y, int font_size)
{
    GUIFrame* f = (GUIFrame*)frame;
    if (!f) {
        fprintf(stderr, "ERROR: Invalid frame pointer in gui_label_new\n");
        return NULL;
    }
    
    gchar* utf8_text = to_utf8(text);
    
    GtkWidget* label = gtk_label_new(utf8_text);
    g_free(utf8_text);
    
    if (font_size > 0) {
        char* markup = g_strdup_printf("<span size='%d'>%s</span>", font_size * 1024, text);
        if (markup) {
            gtk_label_set_markup(GTK_LABEL(label), markup);
            g_free(markup);
        }
    }
    
    gtk_fixed_put(GTK_FIXED(f->fixed), label, x, y);
    gtk_widget_show(label);
    return label;
}

void* gui_button_new(void* frame, const char* text, int x, int y, int w, int h)
{
    GUIFrame* f = (GUIFrame*)frame;
    if (!f) {
        fprintf(stderr, "ERROR: Invalid frame pointer in gui_button_new\n");
        return NULL;
    }
    
    gchar* utf8_text = to_utf8(text);
    
    GtkWidget* button = gtk_button_new_with_label(utf8_text);
    g_free(utf8_text);
    
    gtk_widget_set_size_request(button, w, h);
    gtk_fixed_put(GTK_FIXED(f->fixed), button, x, y);
    gtk_widget_show(button);
    
    return button;
}

void gui_button_set_callback(void* button, const char* callback_str)
{
    if (!button) {
        fprintf(stderr, "ERROR: Invalid button pointer in gui_button_set_callback\n");
        return;
    }
    
    if (callback_debug) {
        printf("DEBUG: gui_button_set_callback called: %s\n", callback_str);
    }
    
    char* cb_str = strdup(callback_str);
    if (!cb_str) {
        fprintf(stderr, "ERROR: Failed to duplicate callback string\n");
        return;
    }
    
    g_signal_connect_data(button, "clicked", G_CALLBACK(on_button_click), cb_str, 
                          (GClosureNotify)free, 0);
}

void gui_label_set_text(void* label, const char* text)
{
    if (!label) return;
    
    gchar* utf8_text = to_utf8(text);
    gtk_label_set_text(GTK_LABEL(label), utf8_text);
    g_free(utf8_text);
}

char* gui_label_get_text(void* label)
{
    if (!label) return NULL;
    
    const gchar* text = gtk_label_get_text(GTK_LABEL(label));
    if (!text) return NULL;
    
    gchar* locale_text = g_locale_from_utf8(text, -1, NULL, NULL, NULL);
    if (!locale_text) {
        return strdup(text);
    }
    
    char* result = strdup(locale_text);
    g_free(locale_text);
    return result;
}

// Empty auto-scaling function - to be implemented later
void gui_auto_widget_scale(void* frame, int enabled)
{
    // TODO: Implement auto-scaling later
    (void)frame;    // Suppress unused parameter warning
    (void)enabled;  // Suppress unused parameter warning
    // Does nothing for now
}

void gui_start(void* frame)
{
    if (!frame) {
        fprintf(stderr, "ERROR: Invalid frame pointer in gui_start\n");
        return;
    }
    
    GUIFrame* f = (GUIFrame*)frame;
    gtk_widget_show_all(f->window);
    gtk_main();
}

void gui_quit(void)
{
    gtk_main_quit();
}

void gui_cleanup(void* frame)
{
    if (frame) {
        GUIFrame* f = (GUIFrame*)frame;
        if (f->window) {
            gtk_widget_destroy(f->window);
        }
        free(frame);
    }
}

void gui_get_widget_size(void* widget, int* width, int* height)
{
    if (!widget) return;
    
    GtkAllocation allocation;
    gtk_widget_get_allocation(GTK_WIDGET(widget), &allocation);
    
    if (width) *width = allocation.width;
    if (height) *height = allocation.height;
}

void gui_set_widget_position(void* widget, int x, int y)
{
    if (!widget) return;
    
    GtkWidget* parent = gtk_widget_get_parent(GTK_WIDGET(widget));
    if (parent && GTK_IS_FIXED(parent)) {
        gtk_fixed_move(GTK_FIXED(parent), GTK_WIDGET(widget), x, y);
    }
}