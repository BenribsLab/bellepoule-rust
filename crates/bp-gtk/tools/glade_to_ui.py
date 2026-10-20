#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Conversion des interfaces Glade GTK2 de BellePoule en fichiers .ui GTK4.

Usage : python glade_to_ui.py <entrée.glade> <sortie.ui> [préfixe-d-actions]

Règles principales (voir aussi `bp_gtk::glade`) :
- les identifiants d'objets sont conservés ;
- GtkHBox/GtkVBox/GtkButtonBox -> GtkBox, GtkTable -> GtkGrid,
  GtkAlignment/GtkEventBox/GtkToolItem -> GtkBox (même identifiant) ;
- GtkToolbar -> GtkBox (classe CSS « toolbar »), boutons d'outils -> GtkButton ;
- GtkMenuBar/GtkMenu -> modèles GMenu (`<menu>`) + GtkPopoverMenuBar ; chaque
  entrée devient une action `<préfixe>.<id>` et garde le nom du gestionnaire
  C++ dans l'attribut `bp-handler` (branché par `Glade`) ;
- les signaux d'événements GTK2 (button-press-event…) deviennent des
  contrôleurs d'événements GTK4 ;
- les signaux de glisser-déposer GTK2 sont retirés (gérés dans le code).
- un widget GTK2 sans `visible=True` reste caché en GTK4 (`visible=False`).
"""

import sys
import xml.etree.ElementTree as ET

IMAGES_DIR = "../glade/images/"

# Icônes et libellés des éléments « stock » GTK2.
STOCK = {
    "gtk-about": ("help-about", "_About"),
    "gtk-add": ("list-add", "_Add"),
    "gtk-apply": ("emblem-ok-symbolic", "_Apply"),
    "gtk-cancel": ("process-stop", "_Cancel"),
    "gtk-clear": ("edit-clear", "_Clear"),
    "gtk-close": ("window-close", "_Close"),
    "gtk-connect": ("network-transmit-receive", "C_onnect"),
    "gtk-convert": ("view-refresh", "_Convert"),
    "gtk-copy": ("edit-copy", "_Copy"),
    "gtk-delete": ("edit-delete", "_Delete"),
    "gtk-dialog-authentication": ("dialog-password", ""),
    "gtk-dialog-info": ("dialog-information", ""),
    "gtk-dialog-warning": ("dialog-warning", ""),
    "gtk-dialog-error": ("dialog-error", ""),
    "gtk-dialog-question": ("dialog-question", ""),
    "gtk-directory": ("folder", ""),
    "gtk-disconnect": ("network-offline", "_Disconnect"),
    "gtk-edit": ("document-edit", "_Edit"),
    "gtk-execute": ("system-run", "_Execute"),
    "gtk-find": ("edit-find", "_Find"),
    "gtk-go-back": ("go-previous", "_Back"),
    "gtk-go-forward": ("go-next", "_Forward"),
    "gtk-go-down": ("go-down", "_Down"),
    "gtk-go-up": ("go-up", "_Up"),
    "gtk-goto-first": ("go-first", "_First"),
    "gtk-goto-last": ("go-last", "_Last"),
    "gtk-harddisk": ("drive-harddisk", "_Harddisk"),
    "gtk-help": ("help-browser", "_Help"),
    "gtk-home": ("go-home", "_Home"),
    "gtk-index": ("format-justify-fill", "_Index"),
    "gtk-info": ("dialog-information", "_Information"),
    "gtk-justify-fill": ("format-justify-fill", "_Fill"),
    "gtk-media-play": ("media-playback-start", "P_lay"),
    "gtk-media-pause": ("media-playback-pause", "P_ause"),
    "gtk-media-stop": ("media-playback-stop", "_Stop"),
    "gtk-missing-image": ("image-missing", ""),
    "gtk-network": ("network-workgroup", "_Network"),
    "gtk-new": ("document-new", "_New"),
    "gtk-no": ("process-stop", "_No"),
    "gtk-ok": ("emblem-ok-symbolic", "_OK"),
    "gtk-open": ("document-open", "_Open"),
    "gtk-page-setup": ("document-page-setup", "Page Set_up"),
    "gtk-paste": ("edit-paste", "_Paste"),
    "gtk-preferences": ("preferences-system", "_Preferences"),
    "gtk-print": ("document-print", "_Print"),
    "gtk-print-preview": ("document-print-preview", "Pre_view"),
    "gtk-properties": ("document-properties", "_Properties"),
    "gtk-quit": ("application-exit", "_Quit"),
    "gtk-redo": ("edit-redo", "_Redo"),
    "gtk-refresh": ("view-refresh", "_Refresh"),
    "gtk-remove": ("list-remove", "_Remove"),
    "gtk-revert-to-saved": ("document-revert", "_Revert"),
    "gtk-save": ("document-save", "_Save"),
    "gtk-save-as": ("document-save-as", "Save _As"),
    "gtk-select-all": ("edit-select-all", "Select _All"),
    "gtk-sort-ascending": ("view-sort-ascending", "_Ascending"),
    "gtk-sort-descending": ("view-sort-descending", "_Descending"),
    "gtk-stop": ("process-stop", "_Stop"),
    "gtk-undo": ("edit-undo", "_Undo"),
    "gtk-yes": ("emblem-ok-symbolic", "_Yes"),
    "gtk-zoom-100": ("zoom-original", "_Normal Size"),
    "gtk-zoom-fit": ("zoom-fit-best", "Best _Fit"),
    "gtk-zoom-in": ("zoom-in", "Zoom _In"),
    "gtk-zoom-out": ("zoom-out", "Zoom _Out"),
    "gtk-ok": ("emblem-ok-symbolic", "_OK"),
    "gtk-media-record": ("media-record", "_Record"),
    "gtk-floppy": ("media-floppy", "_Floppy"),
    "gtk-file": ("text-x-generic", "_File"),
    "gtk-leave-fullscreen": ("view-restore", "_Leave Fullscreen"),
    "gtk-fullscreen": ("view-fullscreen", "_Fullscreen"),
    "gtk-color-picker": ("color-select", ""),
    "gtk-select-color": ("color-select", "_Color"),
    "gtk-spell-check": ("tools-check-spelling", "_Spell Check"),
    "gtk-indent": ("format-indent-more", "_Indent"),
    "gtk-unindent": ("format-indent-less", "_Unindent"),
    "gtk-jump-to": ("go-jump", "_Jump to"),
    "gtk-undelete": ("edit-undelete", "_Undelete"),
    "gtk-bold": ("format-text-bold", "_Bold"),
    "gtk-italic": ("format-text-italic", "_Italic"),
    "gtk-strikethrough": ("format-text-strikethrough", "_Strikethrough"),
    "gtk-underline": ("format-text-underline", "_Underline"),
    "gtk-caps-lock-warning": ("dialog-warning", ""),
    "gtk-cdrom": ("media-optical", "_CD-Rom"),
    "gtk-dnd": ("folder", ""),
    "gtk-dnd-multiple": ("folder", ""),
    "gtk-orientation-landscape": ("object-rotate-right", "Landscape"),
    "gtk-orientation-portrait": ("object-rotate-left", "Portrait"),
}

# Accélérateurs implicites des éléments stock dans les menus GTK2.
STOCK_ACCEL = {
    "gtk-new": "<Control>n",
    "gtk-open": "<Control>o",
    "gtk-save": "<Control>s",
    "gtk-quit": "<Control>q",
    "gtk-print": "<Control>p",
}

RESPONSES = {
    "-1": "none", "-2": "reject", "-3": "accept", "-4": "delete-event",
    "-5": "ok", "-6": "cancel", "-7": "close", "-8": "yes", "-9": "no",
    "-10": "apply", "-11": "help",
}

# Propriétés communes à tous les widgets GTK4, gardées telles quelles.
WIDGET_KEEP = {
    "sensitive", "tooltip_text", "tooltip_markup", "has_tooltip",
    "width_request", "height_request", "receives_default", "halign", "valign",
    "hexpand", "vexpand", "margin_start", "margin_end", "margin_top",
    "margin_bottom", "opacity", "focus_on_click",
}

# Propriétés GTK2 abandonnées (sans équivalent GTK4 ou sans intérêt).
WIDGET_DROP = {
    "can_focus", "has_focus", "is_focus", "can_default", "has_default", "name",
    "no_show_all", "events", "extension_events", "app_paintable",
    "double_buffered", "use_action_appearance", "related_action",
    "resize_mode", "shadow_type", "layout_style", "image_position",
    "always_show_image", "invisible_char", "invisible_char_set",
    "caps_lock_warning", "primary_icon_activatable", "secondary_icon_activatable",
    "primary_icon_sensitive", "secondary_icon_sensitive", "type_hint",
    "skip_taskbar_hint", "skip_pager_hint", "window_position", "border_width",
    "has_separator", "rules_hint", "accel_group", "icon", "urgency_hint",
    "gravity", "focus_on_map", "role", "has_resize_grip", "label_yalign",
    "angle", "track_visited_links", "snap_to_ticks", "update_policy",
    "enable_popup", "right_justified", "draw_as_radio", "foreground_gdk",
    "restrict_to_fill_level", "fill_level", "show_fill_level", "show_arrow",
    "xscale", "yscale", "icon_size", "icon-size", "stock_size", "use_stock",
    "headers_clickable_dummy", "allow_shrink", "allow_grow", "mnemonic_widget",
    "pattern", "do_overwrite_confirmation", "local_only", "preview_widget_active",
    "use_preview_label", "has_frame_dummy", "visible_window", "above_child",
    "truncate_multiline", "overwrite_mode", "im_module", "shadow", "editing_canceled",
    "scrollable", "type",
}

CONTAINERS_WINDOW = {"GtkWindow", "GtkDialog", "GtkMessageDialog", "GtkAboutDialog",
                     "GtkFileChooserDialog", "GtkAssistant"}

EVENT_SIGNALS = {
    "button-press-event": ("GtkGestureClick", "pressed", {"button": "0"}),
    "button_press_event": ("GtkGestureClick", "pressed", {"button": "0"}),
    "button-release-event": ("GtkGestureClick", "released", {"button": "0"}),
    "button_release_event": ("GtkGestureClick", "released", {"button": "0"}),
    "key-press-event": ("GtkEventControllerKey", "key-pressed", {}),
    "key_press_event": ("GtkEventControllerKey", "key-pressed", {}),
    "key-release-event": ("GtkEventControllerKey", "key-released", {}),
    "key_release_event": ("GtkEventControllerKey", "key-released", {}),
    "focus-out-event": ("GtkEventControllerFocus", "leave", {}),
    "focus_out_event": ("GtkEventControllerFocus", "leave", {}),
    "focus-in-event": ("GtkEventControllerFocus", "enter", {}),
    "focus_in_event": ("GtkEventControllerFocus", "enter", {}),
    "enter-notify-event": ("GtkEventControllerMotion", "enter", {}),
    "enter_notify_event": ("GtkEventControllerMotion", "enter", {}),
    "leave-notify-event": ("GtkEventControllerMotion", "leave", {}),
    "leave_notify_event": ("GtkEventControllerMotion", "leave", {}),
    "motion-notify-event": ("GtkEventControllerMotion", "motion", {}),
    "motion_notify_event": ("GtkEventControllerMotion", "motion", {}),
    "scroll-event": ("GtkEventControllerScroll", "scroll", {"flags": "vertical"}),
    "scroll_event": ("GtkEventControllerScroll", "scroll", {"flags": "vertical"}),
}

DROPPED_SIGNALS = {
    "drag-data-received", "drag_data_received", "drag-data-get", "drag_data_get",
    "drag-motion", "drag_motion", "drag-drop", "drag_drop", "drag-leave",
    "drag_leave", "drag-begin", "drag_begin", "drag-end", "drag_end",
    "expose-event", "expose_event", "unmap-event", "unmap_event",
    "map-event", "map_event", "configure-event", "configure_event",
    "window-state-event", "window_state_event", "visibility-notify-event",
}

RENAMED_SIGNALS = {
    "delete-event": "close-request",
    "delete_event": "close-request",
    "released": "clicked",
    "cursor-changed": "cursor-changed",
}


def norm(name):
    return name.replace("-", "_")


def dash(name):
    return name.replace("_", "-")


class Converter:
    def __init__(self, src, prefix):
        self.tree = ET.parse(src)
        self.root = self.tree.getroot()
        self.prefix = prefix
        self.tops = {}
        for o in self.root.findall("object"):
            self.tops[o.get("id")] = o
        self.consumed = set()
        self.menus = []           # éléments <menu> générés
        self.radio_groups = {}    # id d'élément radio -> id du chef de groupe
        self.toolbar_styles = {}
        self.extra_tops = []      # objets GTK4 à ajouter au premier niveau
        self.find_consumed()

    # Objets GtkImage référencés par une propriété (image, icon_widget…) :
    # ils deviennent enfants du bouton qui les utilise.
    def find_consumed(self):
        for el in self.root.iter("property"):
            if norm(el.get("name")) in ("image", "icon_widget", "label_widget"):
                ref = (el.text or "").strip()
                if ref in self.tops:
                    self.consumed.add(ref)

    # ------------------------------------------------------------------
    def props(self, obj):
        out = []
        for p in obj.findall("property"):
            out.append((norm(p.get("name")), p.text or "", dict(p.attrib)))
        return out

    def prop(self, obj, name, default=None):
        for p in obj.findall("property"):
            if norm(p.get("name")) == name:
                return p.text or ""
        return default

    def is_true(self, v):
        return (v or "").strip().lower() in ("true", "1", "yes")

    # ------------------------------------------------------------------
    def new_prop(self, el, name, value, attrs=None):
        p = ET.SubElement(el, "property", {"name": dash(name)})
        if attrs:
            for k in ("translatable", "context", "comments"):
                if k in attrs:
                    p.set(k, attrs[k])
        p.text = value
        return p

    def image_object(self, ref_or_obj):
        """Construit un GtkImage GTK4 à partir d'un GtkImage GTK2."""
        src = self.tops.get(ref_or_obj) if isinstance(ref_or_obj, str) else ref_or_obj
        if src is None:
            return None
        return self.convert_object(src, None, {})

    def stock_icon(self, stock):
        return STOCK.get(stock, (stock.replace("gtk-", ""), ""))[0]

    def stock_label(self, stock):
        return STOCK.get(stock, ("", stock))[1] or stock

    # ------------------------------------------------------------------
    def convert(self):
        out = ET.Element("interface")
        ET.SubElement(out, "requires", {"lib": "gtk", "version": "4.0"})
        body = []
        for o in self.root.findall("object"):
            if o.get("id") in self.consumed:
                continue
            if o.get("class") == "GtkAccelGroup":
                continue
            if o.get("class") in ("GtkMenu",):
                self.menus.append(self.menu_model(o, o.get("id")))
                continue
            new = self.convert_object(o, None, {})
            if new is not None:
                body.append(new)
        for m in self.menus:
            out.append(m)
        for e in self.extra_tops:
            out.append(e)
        for b in body:
            out.append(b)
        return out

    # ------------------------------------------------------------------
    # Modèles de menus
    def menu_label(self, item):
        label = self.prop(item, "label", "")
        attrs = {}
        for p in item.findall("property"):
            if norm(p.get("name")) == "label":
                attrs = dict(p.attrib)
        stock = None
        if self.is_true(self.prop(item, "use_stock")):
            stock = label
            label = self.stock_label(label)
            attrs = {"translatable": "yes"}
        return label, attrs, stock

    def add_attr(self, el, name, value, attrs=None):
        a = ET.SubElement(el, "attribute", {"name": name})
        if attrs:
            for k in ("translatable", "context", "comments"):
                if k in attrs:
                    a.set(k, attrs[k])
        a.text = value
        return a

    def menu_model(self, menu_obj, menu_id):
        m = ET.Element("menu", {"id": menu_id})
        self.fill_menu(m, menu_obj)
        return m

    def fill_menu(self, container, menu_obj):
        section = ET.SubElement(container, "section")
        for c in menu_obj.findall("child"):
            item = c.find("object")
            if item is None:
                continue
            cls = item.get("class")
            iid = item.get("id")
            if cls == "GtkSeparatorMenuItem":
                section = ET.SubElement(container, "section")
                continue
            label, lattrs, stock = self.menu_label(item)
            sub = None
            for cc in item.findall("child"):
                if cc.get("type") == "submenu":
                    sub = cc.find("object")
            if sub is None:
                for p in item.findall("property"):
                    if norm(p.get("name")) == "submenu":
                        sub = self.tops.get(p.text)
            if sub is not None:
                s = ET.SubElement(section, "submenu", {"id": sub.get("id")})
                self.add_attr(s, "label", label, lattrs)
                self.add_attr(s, "bp-id", iid)
                if not self.is_true(self.prop(item, "visible", "False")):
                    self.add_attr(s, "bp-hidden", "true")
                self.fill_menu(s, sub)
                continue
            it = ET.SubElement(section, "item")
            self.add_attr(it, "label", label, lattrs)
            self.add_attr(it, "bp-id", iid)
            handler = None
            for s in item.findall("signal"):
                if s.get("name") in ("activate", "toggled", "button-press-event",
                                     "button_press_event", "activate_item"):
                    handler = s.get("handler")
            action = iid
            if cls in ("GtkRadioMenuItem",):
                leader = self.prop(item, "group") or iid
                action = leader
                self.add_attr(it, "action", "%s.%s" % (self.prefix, dash(action)))
                self.add_attr(it, "target", iid)
                if self.is_true(self.prop(item, "active")):
                    self.add_attr(it, "bp-active", "true")
            elif cls == "GtkCheckMenuItem":
                self.add_attr(it, "action", "%s.%s" % (self.prefix, dash(action)))
                self.add_attr(it, "bp-check", "true")
                if self.is_true(self.prop(item, "active")):
                    self.add_attr(it, "bp-active", "true")
            else:
                self.add_attr(it, "action", "%s.%s" % (self.prefix, dash(action)))
            if handler:
                self.add_attr(it, "bp-handler", handler)
            if not self.is_true(self.prop(item, "visible", "False")):
                self.add_attr(it, "bp-hidden", "true")
            if not self.is_true(self.prop(item, "sensitive", "True")):
                self.add_attr(it, "bp-insensitive", "true")
            accel = None
            for a in item.findall("accelerator"):
                mods = a.get("modifiers", "")
                k = ""
                if "GDK_CONTROL_MASK" in mods:
                    k += "<Control>"
                if "GDK_SHIFT_MASK" in mods:
                    k += "<Shift>"
                if "GDK_MOD1_MASK" in mods:
                    k += "<Alt>"
                accel = k + a.get("key", "").replace("GDK_", "").replace("KEY_", "")
            if accel is None and stock in STOCK_ACCEL:
                accel = STOCK_ACCEL[stock]
            if accel:
                self.add_attr(it, "accel", accel)
            img = self.prop(item, "image")
            if img and img in self.tops:
                stock_img = self.prop(self.tops[img], "stock")
                pix = self.prop(self.tops[img], "pixbuf")
                if stock_img:
                    self.add_attr(it, "bp-icon", self.stock_icon(stock_img))
                elif pix:
                    self.add_attr(it, "bp-icon-file", IMAGES_DIR + pix.replace("images/", ""))
            elif stock:
                self.add_attr(it, "bp-icon", self.stock_icon(stock))
        # Supprime les sections vides
        for s in list(container.findall("section")):
            if len(s) == 0:
                container.remove(s)

    # ------------------------------------------------------------------
    def common_props(self, obj, out, cls, handled):
        """Propriétés communes ; renvoie l'ensemble des noms traités."""
        visible = None
        for (name, value, attrs) in self.props(obj):
            if name in handled:
                continue
            if name == "visible":
                visible = self.is_true(value)
            elif name in WIDGET_KEEP:
                self.new_prop(out, name, value, attrs)
            elif name in WIDGET_DROP:
                pass
            elif name == "xpad":
                if value.strip() not in ("0", ""):
                    self.new_prop(out, "margin_start", value)
                    self.new_prop(out, "margin_end", value)
            elif name == "ypad":
                if value.strip() not in ("0", ""):
                    self.new_prop(out, "margin_top", value)
                    self.new_prop(out, "margin_bottom", value)
            else:
                self.new_prop(out, name, value, attrs)
        if cls not in CONTAINERS_WINDOW and not visible:
            self.new_prop(out, "visible", "False")
        bw = self.prop(obj, "border_width")
        if bw and bw.strip() not in ("0", "") and cls not in CONTAINERS_WINDOW:
            for m in ("margin_start", "margin_end", "margin_top", "margin_bottom"):
                self.new_prop(out, m, bw)

    def signals(self, obj, out):
        controllers = {}
        for s in obj.findall("signal"):
            name = s.get("name")
            handler = s.get("handler")
            if name in DROPPED_SIGNALS:
                out.append(ET.Comment(" GTK2 : %s -> %s (géré dans le code) " % (name, handler)))
                continue
            if name in EVENT_SIGNALS:
                ccls, cname, cprops = EVENT_SIGNALS[name]
                key = ccls
                if key not in controllers:
                    child = ET.Element("child")
                    ctrl = ET.SubElement(child, "object", {"class": ccls})
                    for k, v in cprops.items():
                        self.new_prop(ctrl, k, v)
                    controllers[key] = (child, ctrl)
                ctrl = controllers[key][1]
                attrs = {"name": cname, "handler": handler}
                ET.SubElement(ctrl, "signal", attrs)
                continue
            new_name = RENAMED_SIGNALS.get(name, name)
            attrs = {"name": dash(new_name), "handler": handler}
            if s.get("swapped"):
                attrs["swapped"] = s.get("swapped")
            if s.get("after"):
                attrs["after"] = s.get("after")
            if s.get("object"):
                attrs["object"] = s.get("object")
            ET.SubElement(out, "signal", attrs)
        return [c for (c, _) in controllers.values()]

    # ------------------------------------------------------------------
    def packing(self, child_el):
        p = {}
        pk = child_el.find("packing")
        if pk is not None:
            for q in pk.findall("property"):
                p[norm(q.get("name"))] = q.text or ""
        return p

    def apply_box_packing(self, obj_out, orientation, packing, cls_child):
        expand = packing.get("expand")
        if expand is None:
            expand = "True"  # valeur par défaut de gtk_box_pack_start en GTK2
        fill = packing.get("fill", "True")
        if self.is_true(expand):
            self.set_prop(obj_out, "hexpand" if orientation == "horizontal" else "vexpand", "True")
            if not self.is_true(fill):
                self.set_prop(obj_out, "halign" if orientation == "horizontal" else "valign", "center")
        pad = packing.get("padding")
        if pad and pad.strip() not in ("0", ""):
            if orientation == "horizontal":
                self.set_prop(obj_out, "margin_start", pad)
                self.set_prop(obj_out, "margin_end", pad)
            else:
                self.set_prop(obj_out, "margin_top", pad)
                self.set_prop(obj_out, "margin_bottom", pad)
        if packing.get("pack_type") == "end" and not self.is_true(expand):
            self.set_prop(obj_out, "halign" if orientation == "horizontal" else "valign", "end")
            self.set_prop(obj_out, "hexpand" if orientation == "horizontal" else "vexpand", "True")

    def set_prop(self, el, name, value):
        for p in el.findall("property"):
            if norm(p.get("name")) == norm(name):
                p.text = value
                return
        self.new_prop(el, name, value)

    def get_out_prop(self, el, name):
        for p in el.findall("property"):
            if norm(p.get("name")) == norm(name):
                return p.text
        return None

    # ------------------------------------------------------------------
    def convert_children(self, obj, out, kind, orientation=None):
        """Convertit les enfants d'un conteneur selon son type GTK4."""
        for c in obj.findall("child"):
            internal = c.get("internal-child")
            ctype = c.get("type")
            sub = c.find("object")
            if sub is None:
                continue
            packing = self.packing(c)
            new = self.convert_object(sub, kind, packing)
            if new is None:
                continue
            if isinstance(new, list):
                for n in new:
                    ch = ET.SubElement(out, "child")
                    ch.append(n)
                continue
            if kind == "box":
                self.apply_box_packing(new, orientation, packing, sub.get("class"))
            ch = ET.SubElement(out, "child")
            if ctype == "label" and kind == "frame":
                ch.set("type", "label")
            elif ctype == "tab" and kind == "notebook":
                ch.set("type", "tab")
            elif ctype in ("label",) and kind == "expander":
                ch.set("type", "label")
            ch.append(new)
            if kind == "grid":
                layout = ET.SubElement(new, "layout")
                left = int(packing.get("left_attach", "0"))
                right = int(packing.get("right_attach", str(left + 1)))
                top = int(packing.get("top_attach", "0"))
                bottom = int(packing.get("bottom_attach", str(top + 1)))
                ET.SubElement(layout, "property", {"name": "column"}).text = str(left)
                ET.SubElement(layout, "property", {"name": "row"}).text = str(top)
                if right - left > 1:
                    ET.SubElement(layout, "property", {"name": "column-span"}).text = str(right - left)
                if bottom - top > 1:
                    ET.SubElement(layout, "property", {"name": "row-span"}).text = str(bottom - top)
                xo = packing.get("x_options")
                yo = packing.get("y_options")
                # Options par défaut d'une GtkTable GTK2 : GTK_EXPAND | GTK_FILL
                if xo is None or "GTK_EXPAND" in xo:
                    self.set_prop(new, "hexpand", "True")
                if yo is None or "GTK_EXPAND" in yo:
                    self.set_prop(new, "vexpand", "True")
                if xo is not None and "GTK_FILL" not in xo and self.get_out_prop(new, "halign") is None:
                    self.set_prop(new, "halign", "start")
                if yo is not None and "GTK_FILL" not in yo and self.get_out_prop(new, "valign") is None:
                    self.set_prop(new, "valign", "center")
                xp = packing.get("x_padding")
                if xp and xp.strip() != "0":
                    self.set_prop(new, "margin_start", xp)
                    self.set_prop(new, "margin_end", xp)
                yp = packing.get("y_padding")
                if yp and yp.strip() != "0":
                    self.set_prop(new, "margin_top", yp)
                    self.set_prop(new, "margin_bottom", yp)
                # <layout> doit suivre les propriétés
                new.remove(layout)
                new.append(layout)

    # ------------------------------------------------------------------
    def convert_object(self, obj, parent_kind, packing):
        cls = obj.get("class")
        oid = obj.get("id")
        fn = getattr(self, "c_" + cls, None)
        if fn is None:
            fn = self.c_default
        return fn(obj, cls, oid)

    def new_obj(self, cls, oid):
        e = ET.Element("object", {"class": cls})
        if oid:
            e.set("id", oid)
        return e

    def finish(self, obj, out, cls, handled=()):
        self.common_props(obj, out, cls, set(handled))
        for ctrl in self.signals(obj, out):
            out.append(ctrl)
        if cls in ("GtkAboutDialog", "GtkWindow"):
            for s in out.findall("signal"):
                if s.get("name") == "close":
                    s.set("name", "close-request")
        # Les accélérateurs de widgets sont gérés dans le code en GTK4.
        return out

    # --- classes -------------------------------------------------------
    def c_default(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls)
        self.convert_children(obj, out, "bin")
        return out

    def c_GtkAdjustment(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        for (n, v, a) in self.props(obj):
            self.new_prop(out, n, v)
        return out

    def store(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        for el in obj:
            if el.tag == "columns":
                cols = ET.SubElement(out, "columns")
                for col in el.findall("column"):
                    t = col.get("type")
                    ET.SubElement(cols, "column", {"type": t})
            elif el.tag == "data":
                out.append(el)
            elif el.tag == "property":
                self.new_prop(out, el.get("name"), el.text or "")
        return out

    c_GtkListStore = store
    c_GtkTreeStore = store

    def c_GtkTreeModelFilter(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        for (n, v, a) in self.props(obj):
            self.new_prop(out, n, v)
        return out

    def c_GtkSizeGroup(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        for (n, v, a) in self.props(obj):
            if n in ("mode",):
                self.new_prop(out, n, v)
        w = obj.find("widgets")
        if w is not None:
            out.append(w)
        return out

    def c_GtkCellRendererText(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        for (n, v, a) in self.props(obj):
            if n in ("stock_id",):
                self.new_prop(out, "icon_name", self.stock_icon(v))
            elif n.endswith("_gdk"):
                self.new_prop(out, n[:-4], v)
            elif n in ("stock_size",):
                pass
            else:
                self.new_prop(out, n, v, a)
        for ctrl in self.signals(obj, out):
            pass
        return out

    c_GtkCellRendererPixbuf = c_GtkCellRendererText
    c_GtkCellRendererToggle = c_GtkCellRendererText
    c_GtkCellRendererCombo = c_GtkCellRendererText
    c_GtkCellRendererProgress = c_GtkCellRendererText
    c_GtkCellRendererSpin = c_GtkCellRendererText

    def cell_layout_children(self, obj, out):
        for c in obj.findall("child"):
            sub = c.find("object")
            if sub is None:
                continue
            ch = ET.SubElement(out, "child")
            ch.append(self.convert_object(sub, "cell", {}))
            attrs = c.find("attributes")
            if attrs is not None:
                na = ET.SubElement(ch, "attributes")
                for a in attrs.findall("attribute"):
                    name = a.get("name")
                    if name == "stock_id" or name == "stock-id":
                        name = "icon-name"
                    if name in ("foreground_gdk", "background_gdk", "cell_background_gdk"):
                        name = name.replace("_gdk", "_rgba")
                    ET.SubElement(na, "attribute", {"name": dash(name)}).text = a.text

    def c_GtkTreeViewColumn(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        for (n, v, a) in self.props(obj):
            if n in ("visible",):
                if not self.is_true(v):
                    self.new_prop(out, n, v)
            elif n in WIDGET_DROP:
                pass
            else:
                self.new_prop(out, n, v, a)
        for s in obj.findall("signal"):
            ET.SubElement(out, "signal", {"name": dash(s.get("name")), "handler": s.get("handler")})
        self.cell_layout_children(obj, out)
        return out

    def c_GtkTreeView(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"headers_clickable"}
        self.finish(obj, out, cls, handled)
        hc = self.prop(obj, "headers_clickable")
        if hc is not None:
            self.new_prop(out, "headers_clickable", hc)
        for c in obj.findall("child"):
            sub = c.find("object")
            if sub is not None:
                ch = ET.SubElement(out, "child")
                ch.append(self.convert_object(sub, "treeview", {}))
        return out

    def c_GtkComboBox(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls)
        self.cell_layout_children(obj, out)
        if self.is_true(self.prop(obj, "has_entry")):
            pass
        return out

    def c_GtkComboBoxText(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls)
        items = obj.find("items")
        if items is not None:
            out.append(items)
        return out

    def c_GtkWindow(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls)
        self.convert_children(obj, out, "bin")
        return out

    def c_GtkLabel(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls, handled={"justify"})
        j = self.prop(obj, "justify")
        if j:
            self.new_prop(out, "justify", j)
        attrs = obj.find("attributes")
        if attrs is not None:
            out.append(attrs)
        return out

    def c_GtkImage(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"stock", "pixbuf", "icon_size", "icon-size", "xalign", "yalign",
                   "pixel_size", "icon_name", "file"}
        self.finish(obj, out, cls, handled)
        stock = self.prop(obj, "stock")
        pix = self.prop(obj, "pixbuf")
        icon_name = self.prop(obj, "icon_name")
        size = self.prop(obj, "icon_size") or self.prop(obj, "icon-size")
        if stock:
            self.new_prop(out, "icon_name", self.stock_icon(stock))
        elif icon_name:
            self.new_prop(out, "icon_name", icon_name)
        elif pix:
            self.new_prop(out, "file", IMAGES_DIR + pix.replace("images/", ""))
            self.new_prop(out, "pixel_size", "-1")
        ps = self.prop(obj, "pixel_size")
        if ps and ps.strip() not in ("-1", "0"):
            self.new_prop(out, "pixel_size", ps)
        if size and size.strip() in ("5", "6"):
            self.new_prop(out, "icon_size", "large")
        return out

    def image_child_for(self, ref):
        if ref and ref in self.tops:
            return self.convert_object(self.tops[ref], None, {})
        return None

    def button_content(self, obj, out, label, use_stock, image_ref, children):
        """Contenu d'un bouton GTK4 : icône et/ou libellé."""
        icon = None
        if use_stock and label:
            icon = self.stock_icon(label)
            label = self.stock_label(label)
        img = self.image_child_for(image_ref)
        if children:
            return children, None
        if img is not None and label:
            box = self.new_obj("GtkBox", None)
            self.new_prop(box, "spacing", "4")
            ch = ET.SubElement(box, "child")
            ch.append(img)
            lab = self.new_obj("GtkLabel", None)
            self.new_prop(lab, "label", label, {"translatable": "yes"})
            self.new_prop(lab, "use_underline", "True")
            ch = ET.SubElement(box, "child")
            ch.append(lab)
            return [box], None
        if img is not None:
            return [img], None
        if icon and label:
            box = self.new_obj("GtkBox", None)
            self.new_prop(box, "spacing", "4")
            im = self.new_obj("GtkImage", None)
            self.new_prop(im, "icon_name", icon)
            ch = ET.SubElement(box, "child")
            ch.append(im)
            lab = self.new_obj("GtkLabel", None)
            self.new_prop(lab, "label", label, {"translatable": "yes"})
            self.new_prop(lab, "use_underline", "True")
            ch = ET.SubElement(box, "child")
            ch.append(lab)
            return [box], None
        return [], label

    def c_GtkButton(self, obj, cls, oid, new_cls=None):
        out = self.new_obj(new_cls or cls, oid)
        handled = {"label", "use_stock", "image", "use_underline", "relief", "xalign", "yalign",
                   "draw_indicator", "group", "active", "inconsistent", "uri"}
        self.finish(obj, out, cls, handled)
        label = self.prop(obj, "label")
        use_stock = self.is_true(self.prop(obj, "use_stock"))
        children = []
        for c in obj.findall("child"):
            sub = c.find("object")
            if sub is not None:
                children.append(self.convert_object(sub, "bin", {}))
        content, text = self.button_content(obj, out, label, use_stock,
                                            self.prop(obj, "image"), children)
        if text is not None and text != "":
            attrs = {}
            for p in obj.findall("property"):
                if norm(p.get("name")) == "label":
                    attrs = dict(p.attrib)
            if use_stock:
                attrs = {"translatable": "yes"}
            self.new_prop(out, "label", text, attrs)
            if use_stock or self.is_true(self.prop(obj, "use_underline")):
                self.new_prop(out, "use_underline", "True")
        if self.prop(obj, "relief") == "none":
            self.new_prop(out, "has_frame", "False")
        if self.prop(obj, "uri"):
            self.new_prop(out, "uri", self.prop(obj, "uri"))
        for k in ("active", "inconsistent"):
            v = self.prop(obj, k)
            if v is not None and (new_cls or cls) in ("GtkCheckButton", "GtkToggleButton"):
                self.new_prop(out, k, v)
        g = self.prop(obj, "group")
        if g and (new_cls or cls) in ("GtkCheckButton", "GtkToggleButton"):
            self.new_prop(out, "group", g)
        for c in content:
            if (new_cls or cls) == "GtkCheckButton":
                ch = ET.SubElement(out, "property", {"name": "child"})
            else:
                ch = ET.SubElement(out, "child")
            ch.append(c)
        if (new_cls or cls) in ("GtkCheckButton", "GtkToggleButton"):
            for s in out.findall("signal"):
                if s.get("name") == "clicked":
                    s.set("name", "toggled")
        return out

    def c_GtkLinkButton(self, obj, cls, oid):
        return self.c_GtkButton(obj, cls, oid)

    def c_GtkToggleButton(self, obj, cls, oid):
        return self.c_GtkButton(obj, cls, oid, "GtkToggleButton")

    def c_GtkCheckButton(self, obj, cls, oid):
        draw = self.prop(obj, "draw_indicator")
        if draw is not None and not self.is_true(draw):
            return self.c_GtkButton(obj, cls, oid, "GtkToggleButton")
        return self.c_GtkButton(obj, cls, oid, "GtkCheckButton")

    def c_GtkRadioButton(self, obj, cls, oid):
        # Un groupe GTK4 est défini par la propriété « group » des membres.
        draw = self.prop(obj, "draw_indicator")
        if draw is not None and not self.is_true(draw):
            return self.c_GtkButton(obj, cls, oid, "GtkToggleButton")
        return self.c_GtkButton(obj, cls, oid, "GtkCheckButton")

    def c_GtkSpinButton(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"max_length", "width_chars", "text", "xalign", "editable",
                   "secondary_icon_stock", "primary_icon_stock", "visibility",
                   "has_frame", "truncate_multiline"}
        self.finish(obj, out, cls, handled)
        for k in ("width_chars", "max_length", "xalign"):
            pass
        return out

    def c_GtkEntry(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"secondary_icon_stock", "primary_icon_stock"}
        self.finish(obj, out, cls, handled)
        for k, n in (("secondary_icon_stock", "secondary_icon_name"),
                     ("primary_icon_stock", "primary_icon_name")):
            v = self.prop(obj, k)
            if v:
                self.new_prop(out, n, self.stock_icon(v))
        return out

    def c_GtkScrolledWindow(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"window_placement", "hadjustment", "vadjustment"}
        self.finish(obj, out, cls, handled)
        st = self.prop(obj, "shadow_type")
        if st and st != "none":
            self.new_prop(out, "has_frame", "True")
        self.convert_children(obj, out, "bin")
        return out

    def c_GtkViewport(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls, {"hadjustment", "vadjustment"})
        self.convert_children(obj, out, "bin")
        return out

    def box(self, obj, cls, oid, orientation):
        out = self.new_obj("GtkBox", oid)
        handled = {"orientation", "layout_style"}
        self.new_prop(out, "orientation", orientation)
        self.finish(obj, out, cls, handled)
        ls = self.prop(obj, "layout_style")
        if cls.endswith("ButtonBox"):
            if self.get_out_prop(out, "spacing") is None:
                self.new_prop(out, "spacing", "6")
            if ls in ("end", None):
                self.new_prop(out, "halign" if orientation == "horizontal" else "valign", "end")
            elif ls == "center":
                self.new_prop(out, "halign" if orientation == "horizontal" else "valign", "center")
            elif ls == "spread" or ls == "edge":
                self.new_prop(out, "homogeneous", "True")
        self.convert_children(obj, out, "box", orientation)
        return out

    def c_GtkHBox(self, obj, cls, oid):
        return self.box(obj, cls, oid, "horizontal")

    def c_GtkVBox(self, obj, cls, oid):
        return self.box(obj, cls, oid, "vertical")

    def c_GtkBox(self, obj, cls, oid):
        return self.box(obj, cls, oid, self.prop(obj, "orientation", "horizontal"))

    def c_GtkHButtonBox(self, obj, cls, oid):
        return self.box(obj, cls, oid, "horizontal")

    def c_GtkVButtonBox(self, obj, cls, oid):
        return self.box(obj, cls, oid, "vertical")

    def c_GtkHSeparator(self, obj, cls, oid):
        out = self.new_obj("GtkSeparator", oid)
        self.new_prop(out, "orientation", "horizontal")
        self.finish(obj, out, cls)
        return out

    def c_GtkVSeparator(self, obj, cls, oid):
        out = self.new_obj("GtkSeparator", oid)
        self.new_prop(out, "orientation", "vertical")
        self.finish(obj, out, cls)
        return out

    def c_GtkHScale(self, obj, cls, oid):
        out = self.new_obj("GtkScale", oid)
        self.new_prop(out, "orientation", "horizontal")
        self.finish(obj, out, cls)
        return out

    def c_GtkVScale(self, obj, cls, oid):
        out = self.new_obj("GtkScale", oid)
        self.new_prop(out, "orientation", "vertical")
        self.finish(obj, out, cls)
        return out

    def c_GtkHPaned(self, obj, cls, oid):
        out = self.new_obj("GtkPaned", oid)
        self.new_prop(out, "orientation", "horizontal")
        self.finish(obj, out, cls)
        self.paned_children(obj, out)
        return out

    def c_GtkVPaned(self, obj, cls, oid):
        out = self.new_obj("GtkPaned", oid)
        self.new_prop(out, "orientation", "vertical")
        self.finish(obj, out, cls)
        self.paned_children(obj, out)
        return out

    def paned_children(self, obj, out):
        names = ["start_child", "end_child"]
        i = 0
        for c in obj.findall("child"):
            sub = c.find("object")
            if sub is None or i > 1:
                continue
            p = ET.SubElement(out, "property", {"name": dash(names[i])})
            p.append(self.convert_object(sub, "bin", {}))
            i += 1

    def c_GtkTable(self, obj, cls, oid):
        out = self.new_obj("GtkGrid", oid)
        handled = {"n_rows", "n_columns", "homogeneous", "row_spacing", "column_spacing"}
        self.finish(obj, out, cls, handled)
        for k in ("row_spacing", "column_spacing"):
            v = self.prop(obj, k)
            if v:
                self.new_prop(out, k, v)
        if self.is_true(self.prop(obj, "homogeneous")):
            self.new_prop(out, "row_homogeneous", "True")
            self.new_prop(out, "column_homogeneous", "True")
        self.convert_children(obj, out, "grid")
        return out

    def alignment_like(self, obj, cls, oid):
        """GtkAlignment / GtkEventBox / GtkToolItem -> GtkBox de même id."""
        out = self.new_obj("GtkBox", oid)
        handled = {"xalign", "yalign", "xscale", "yscale", "top_padding",
                   "bottom_padding", "left_padding", "right_padding",
                   "visible_window", "above_child", "is_important",
                   "visible_horizontal", "visible_vertical", "homogeneous"}
        self.new_prop(out, "orientation", "vertical")
        self.finish(obj, out, cls, handled)
        for k, m in (("top_padding", "margin_top"), ("bottom_padding", "margin_bottom"),
                     ("left_padding", "margin_start"), ("right_padding", "margin_end")):
            v = self.prop(obj, k)
            if v and v.strip() != "0":
                self.set_prop(out, m, v)
        xscale = float(self.prop(obj, "xscale", "1") or 1)
        yscale = float(self.prop(obj, "yscale", "1") or 1)
        xalign = float(self.prop(obj, "xalign", "0.5") or 0.5)
        yalign = float(self.prop(obj, "yalign", "0.5") or 0.5)
        for c in obj.findall("child"):
            sub = c.find("object")
            if sub is None:
                continue
            new = self.convert_object(sub, "bin", {})
            if cls == "GtkAlignment":
                if xscale < 1:
                    self.set_prop(new, "halign", "start" if xalign < 0.3 else ("end" if xalign > 0.7 else "center"))
                else:
                    self.set_prop(new, "hexpand", "True")
                if yscale < 1:
                    self.set_prop(new, "valign", "start" if yalign < 0.3 else ("end" if yalign > 0.7 else "center"))
                else:
                    self.set_prop(new, "vexpand", "True")
            else:
                self.set_prop(new, "hexpand", "True")
                self.set_prop(new, "vexpand", "True")
            ch = ET.SubElement(out, "child")
            ch.append(new)
        return out

    c_GtkAlignment = alignment_like
    c_GtkEventBox = alignment_like
    c_GtkToolItem = alignment_like

    def c_GtkFrame(self, obj, cls, oid):
        out = self.new_obj("GtkFrame", oid)
        handled = {"label_xalign", "label_yalign", "ratio", "obey_child", "xalign", "yalign",
                   "label", "label_widget"}
        self.finish(obj, out, cls, handled)
        lx = self.prop(obj, "label_xalign")
        if lx:
            self.new_prop(out, "label_xalign", lx)
        lab = self.prop(obj, "label")
        if lab:
            self.new_prop(out, "label", lab, {"translatable": "yes"})
        self.convert_children(obj, out, "frame")
        st = self.prop(obj, "shadow_type")
        if st == "none" and not obj.findall("child[@type='label']"):
            # Cadre sans bordure : pas d'équivalent direct, on garde le cadre
            out.set("class", "GtkFrame")
        return out

    c_GtkAspectFrame = c_GtkFrame

    def c_GtkExpander(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls)
        self.convert_children(obj, out, "expander")
        return out

    def c_GtkNotebook(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"tab_hborder", "tab_vborder", "tab_border", "homogeneous", "page"}
        self.finish(obj, out, cls, handled)
        self.convert_children(obj, out, "notebook")
        return out

    def c_GtkSpinner(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        self.finish(obj, out, cls, {"active"})
        if self.is_true(self.prop(obj, "active")):
            self.new_prop(out, "spinning", "True")
        return out

    def c_GtkCalendar(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"year", "month", "day", "display_options"}
        self.finish(obj, out, cls, handled)
        return out

    # --- Barres d'outils ----------------------------------------------
    def c_GtkToolbar(self, obj, cls, oid):
        out = self.new_obj("GtkBox", oid)
        handled = {"toolbar_style", "show_arrow", "icon_size", "orientation"}
        style = self.prop(obj, "toolbar_style", "both")
        orientation = self.prop(obj, "orientation", "horizontal")
        self.new_prop(out, "orientation", orientation)
        self.finish(obj, out, cls, handled)
        st = ET.SubElement(out, "style")
        ET.SubElement(st, "class", {"name": "toolbar"})
        for c in obj.findall("child"):
            sub = c.find("object")
            if sub is None:
                continue
            packing = self.packing(c)
            new = self.tool_item(sub, style, orientation)
            if new is None:
                continue
            if self.is_true(packing.get("expand")):
                self.set_prop(new, "hexpand" if orientation == "horizontal" else "vexpand", "True")
            ch = ET.SubElement(out, "child")
            ch.append(new)
        return out

    def tool_item(self, obj, style, orientation):
        cls = obj.get("class")
        oid = obj.get("id")
        if cls == "GtkSeparatorToolItem":
            draw = self.prop(obj, "draw")
            if draw is not None and not self.is_true(draw):
                out = self.new_obj("GtkBox", oid)
            else:
                out = self.new_obj("GtkSeparator", oid)
                self.new_prop(out, "orientation", "vertical" if orientation == "horizontal" else "horizontal")
            self.finish(obj, out, cls, {"draw"})
            return out
        if cls in ("GtkToolButton", "GtkToggleToolButton", "GtkMenuToolButton", "GtkRadioToolButton"):
            new_cls = {"GtkToolButton": "GtkButton",
                       "GtkToggleToolButton": "GtkToggleButton",
                       "GtkRadioToolButton": "GtkToggleButton",
                       "GtkMenuToolButton": "GtkMenuButton"}[cls]
            out = self.new_obj(new_cls, oid)
            handled = {"label", "stock_id", "icon_widget", "label_widget", "use_underline",
                       "is_important", "visible_horizontal", "visible_vertical", "icon_name",
                       "active", "group"}
            self.finish(obj, out, cls, handled)
            self.new_prop(out, "has_frame", "False")
            if new_cls == "GtkMenuButton":
                # GtkMenuButton n'a pas de signal « clicked »
                for s in list(out.findall("signal")):
                    if s.get("name") == "clicked":
                        out.remove(s)
            if self.prop(obj, "active") is not None and new_cls == "GtkToggleButton":
                self.new_prop(out, "active", self.prop(obj, "active"))
            if self.prop(obj, "group") and new_cls == "GtkToggleButton":
                self.new_prop(out, "group", self.prop(obj, "group"))
            label = self.prop(obj, "label")
            label_attrs = {}
            for p in obj.findall("property"):
                if norm(p.get("name")) == "label":
                    label_attrs = dict(p.attrib)
            stock = self.prop(obj, "stock_id")
            icon_name = self.prop(obj, "icon_name")
            if stock and not label:
                label = self.stock_label(stock)
                label_attrs = {"translatable": "yes"}
            img = None
            iw = self.prop(obj, "icon_widget")
            if iw:
                img = self.image_child_for(iw)
            elif stock:
                img = self.new_obj("GtkImage", None)
                self.new_prop(img, "icon_name", self.stock_icon(stock))
                self.new_prop(img, "icon_size", "large")
            elif icon_name:
                img = self.new_obj("GtkImage", None)
                self.new_prop(img, "icon_name", icon_name)
                self.new_prop(img, "icon_size", "large")
            if img is not None:
                vis = self.get_out_prop(img, "visible")
                if vis is not None:
                    img.remove([p for p in img.findall("property") if p.get("name") == "visible"][0])
            show_label = style in ("both", "both_horiz", "both-horiz", "text") and label
            if style == "icons" and img is not None:
                show_label = False
            if img is None and label:
                show_label = True
            if not self.get_out_prop(out, "tooltip_text") and label:
                self.new_prop(out, "tooltip_text", label.replace("_", ""), label_attrs)
            if show_label and img is not None:
                box = self.new_obj("GtkBox", None)
                self.new_prop(box, "orientation", "vertical" if style == "both" else "horizontal")
                self.new_prop(box, "spacing", "2")
                ch = ET.SubElement(box, "child")
                ch.append(img)
                lab = self.new_obj("GtkLabel", None)
                self.new_prop(lab, "label", label, label_attrs)
                self.new_prop(lab, "use_underline", "True")
                ch = ET.SubElement(box, "child")
                ch.append(lab)
                content = box
            elif img is not None:
                content = img
            else:
                content = self.new_obj("GtkLabel", None)
                self.new_prop(content, "label", label or "", label_attrs)
                self.new_prop(content, "use_underline", "True")
            ch = ET.SubElement(out, "child")
            ch.append(content)
            return out
        if cls == "GtkToolItem":
            return self.alignment_like(obj, cls, oid)
        return self.convert_object(obj, "box", {})

    # --- Barre de menus -----------------------------------------------
    def c_GtkMenuBar(self, obj, cls, oid):
        model_id = oid + "_model"
        m = ET.Element("menu", {"id": model_id})
        self.fill_menu_bar(m, obj)
        self.menus.append(m)
        out = self.new_obj("GtkPopoverMenuBar", oid)
        self.new_prop(out, "menu_model", model_id)
        return out

    def fill_menu_bar(self, container, bar):
        # Une barre de menus est une suite de sous-menus.
        fake = ET.Element("object", {"class": "GtkMenu"})
        for c in bar.findall("child"):
            fake.append(c)
        self.fill_menu(container, fake)

    # --- Dialogues ------------------------------------------------------
    def dialog(self, obj, cls, oid):
        out = self.new_obj(cls, oid)
        handled = {"has_separator", "message_type", "buttons", "text", "use_markup",
                   "secondary_text", "secondary_use_markup", "image", "authors",
                   "documenters", "artists", "logo", "translator_credits",
                   "program_name", "version", "copyright", "comments", "website",
                   "website_label", "license", "wrap_license", "logo_icon_name",
                   "action", "do_overwrite_confirmation", "select_multiple",
                   "create_folders", "show_hidden", "local_only"}
        self.finish(obj, out, cls, handled)
        if cls == "GtkMessageDialog":
            for k in ("message_type", "buttons", "text", "use_markup", "secondary_text",
                      "secondary_use_markup"):
                for p in obj.findall("property"):
                    if norm(p.get("name")) == k:
                        self.new_prop(out, k, p.text or "", dict(p.attrib))
        if cls == "GtkAboutDialog":
            for k in ("program_name", "version", "copyright", "comments", "website",
                      "website_label", "license", "wrap_license", "authors",
                      "documenters", "artists", "translator_credits"):
                for p in obj.findall("property"):
                    if norm(p.get("name")) == k:
                        self.new_prop(out, k, p.text or "", dict(p.attrib))
            logo = self.prop(obj, "logo")
            if logo:
                self.new_prop(out, "logo", IMAGES_DIR + logo.replace("images/", ""))
        if cls == "GtkFileChooserDialog":
            for k in ("action", "select_multiple", "create_folders"):
                v = self.prop(obj, k)
                if v is not None:
                    self.new_prop(out, k, v)
        # Zone de contenu et zone d'action
        actions = []
        for c in obj.findall("child"):
            internal = c.get("internal-child")
            sub = c.find("object")
            if sub is None:
                continue
            if internal == "vbox" and cls != "GtkAboutDialog":
                content = self.new_obj("GtkBox", sub.get("id"))
                self.new_prop(content, "orientation", "vertical")
                sp = self.prop(sub, "spacing")
                if sp:
                    self.new_prop(content, "spacing", sp)
                for cc in sub.findall("child"):
                    sub2 = cc.find("object")
                    if sub2 is None:
                        continue
                    if cc.get("internal-child") == "action_area":
                        for ac in sub2.findall("child"):
                            b = ac.find("object")
                            if b is not None:
                                actions.append(self.convert_object(b, "box", {}))
                        continue
                    if cc.get("internal-child"):
                        continue
                    packing = self.packing(cc)
                    new = self.convert_object(sub2, "box", packing)
                    if new is None:
                        continue
                    self.apply_box_packing(new, "vertical", packing, sub2.get("class"))
                    ch = ET.SubElement(content, "child")
                    ch.append(new)
                area = "message_area" if False else "content_area"
                ch = ET.SubElement(out, "child", {"internal-child": area})
                ch.append(content)
            elif internal is None:
                # Enfant non interne (rare) : ajouté au contenu
                pass
        for a in actions:
            ch = ET.SubElement(out, "child", {"type": "action"})
            ch.append(a)
        aw = obj.find("action-widgets")
        if aw is not None:
            naw = ET.SubElement(out, "action-widgets")
            ids = {a.get("id") for a in actions}
            for w in aw.findall("action-widget"):
                if w.text not in ids:
                    continue
                r = w.get("response")
                attrs = {"response": RESPONSES.get(r, r)}
                if w.get("default"):
                    attrs["default"] = w.get("default")
                ET.SubElement(naw, "action-widget", attrs).text = w.text
            if len(naw) == 0:
                out.remove(naw)
        return out

    c_GtkDialog = dialog
    c_GtkMessageDialog = dialog
    c_GtkAboutDialog = dialog
    c_GtkFileChooserDialog = dialog


def indent(elem, level=0):
    i = "\n" + level * "  "
    if len(elem):
        if not elem.text or not elem.text.strip():
            elem.text = i + "  "
        for e in elem:
            indent(e, level + 1)
        if not e.tail or not e.tail.strip():
            e.tail = i
    if level and (not elem.tail or not elem.tail.strip()):
        elem.tail = i


def main():
    src, dst = sys.argv[1], sys.argv[2]
    prefix = sys.argv[3] if len(sys.argv) > 3 else "module"
    conv = Converter(src, prefix)
    out = conv.convert()
    indent(out)
    data = ET.tostring(out, encoding="unicode")
    header = ('<?xml version="1.0" encoding="UTF-8"?>\n'
              '<!-- Converti depuis resources/glade/%s (GTK2) vers GTK4.\n'
              '     Préfixe des actions de menus : %s -->\n') % (src.replace("\\", "/").split("/")[-1], prefix)
    with open(dst, "w", encoding="utf-8", newline="\n") as f:
        f.write(header + data + "\n")


if __name__ == "__main__":
    main()
