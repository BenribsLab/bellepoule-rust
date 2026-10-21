//! Contexte d'impression partagé par le canevas et les phases.

#![allow(deprecated)]

use gtk::prelude::*;
use gtk4 as gtk;

use crate::canvas::{Anchor, Canvas, ItemId, Prop};

/// Dimensions d'une page (équivalent de `GtkPrintContext`), en unités de
/// cairo (points d'imprimante).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageContext {
    pub width: f64,
    pub height: f64,
    pub dpi_x: f64,
    pub dpi_y: f64,
}

/// En-tête des pages imprimées (`Contest::DrawPage`) : bandeau à la couleur
/// de la compétition, nom, arme - sexe - catégorie, nom de la page,
/// organisateur et date.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageHeader {
    pub name: String,
    /// « Épée - Hommes - M5 ».
    pub subtitle: String,
    pub organizer: String,
    pub date: String,
    /// Couleur de la compétition (`#rrggbb`).
    pub color: String,
}

/// Mise en page des impressions, réglable par l'utilisateur depuis l'aperçu.
/// Les champs vides reprennent les valeurs de la compétition.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintLayout {
    pub show_header: bool,
    /// Titre du bandeau (vide : nom de la compétition).
    pub title: String,
    /// Couleur du bandeau (vide : couleur de la compétition).
    pub header_color: String,
    pub header_text_color: String,
    /// Couleur de « arme - sexe - catégorie » (ombre noire comme le C++).
    pub subtitle_color: String,
    /// Taille des textes du bandeau, en % (100 : taille du C++).
    pub header_font: u32,
    /// Taille des textes des feuilles, en %.
    pub body_font: u32,
    /// Cases grisées des feuilles de poule.
    pub grey_color: String,
    /// Noms des tireurs.
    pub names_color: String,
    pub show_footer: bool,
    /// Texte du pied de page (vide : mention BellePoule).
    pub footer_text: String,
    pub show_page_numbers: bool,
}

impl Default for PrintLayout {
    fn default() -> Self {
        PrintLayout {
            show_header: true,
            title: String::new(),
            header_color: String::new(),
            header_text_color: "#000000".into(),
            subtitle_color: "#ffffff".into(),
            header_font: 100,
            body_font: 100,
            grey_color: "#bebebe".into(),
            names_color: "#00008b".into(),
            show_footer: true,
            footer_text: String::new(),
            show_page_numbers: true,
        }
    }
}

const LAYOUT_GROUP: &str = "PrintLayout";

impl PrintLayout {
    pub fn load(config: &bp_core::tournament::user_config::UserConfig) -> Self {
        let d = PrintLayout::default();
        let s = |k: &str, def: &str| config.get_string(LAYOUT_GROUP, k).unwrap_or_else(|| def.to_string());
        let b = |k: &str, def: bool| config.get_string(LAYOUT_GROUP, k).map(|v| v == "true").unwrap_or(def);
        let n = |k: &str, def: u32| {
            config.get_string(LAYOUT_GROUP, k).and_then(|v| v.parse::<u32>().ok()).filter(|v| (30..=300).contains(v)).unwrap_or(def)
        };
        PrintLayout {
            show_header: b("show_header", d.show_header),
            title: s("title", &d.title),
            header_color: s("header_color", &d.header_color),
            header_text_color: s("header_text_color", &d.header_text_color),
            subtitle_color: s("subtitle_color", &d.subtitle_color),
            header_font: n("header_font", d.header_font),
            body_font: n("body_font", d.body_font),
            grey_color: s("grey_color", &d.grey_color),
            names_color: s("names_color", &d.names_color),
            show_footer: b("show_footer", d.show_footer),
            footer_text: s("footer_text", &d.footer_text),
            show_page_numbers: b("show_page_numbers", d.show_page_numbers),
        }
    }

    pub fn save(&self, config: &mut bp_core::tournament::user_config::UserConfig) {
        let bool_image = |v: bool| if v { "true" } else { "false" };
        config.set_string(LAYOUT_GROUP, "show_header", bool_image(self.show_header));
        config.set_string(LAYOUT_GROUP, "title", &self.title);
        config.set_string(LAYOUT_GROUP, "header_color", &self.header_color);
        config.set_string(LAYOUT_GROUP, "header_text_color", &self.header_text_color);
        config.set_string(LAYOUT_GROUP, "subtitle_color", &self.subtitle_color);
        config.set_string(LAYOUT_GROUP, "header_font", &self.header_font.to_string());
        config.set_string(LAYOUT_GROUP, "body_font", &self.body_font.to_string());
        config.set_string(LAYOUT_GROUP, "grey_color", &self.grey_color);
        config.set_string(LAYOUT_GROUP, "names_color", &self.names_color);
        config.set_string(LAYOUT_GROUP, "show_footer", bool_image(self.show_footer));
        config.set_string(LAYOUT_GROUP, "footer_text", &self.footer_text);
        config.set_string(LAYOUT_GROUP, "show_page_numbers", bool_image(self.show_page_numbers));
    }

    /// Facteur à appliquer aux tailles de police des feuilles.
    pub fn body_scale(&self) -> f64 {
        self.body_font as f64 / 100.0
    }
}

type HeaderProvider = Box<dyn Fn() -> Option<PageHeader>>;
type LayoutSaver = Box<dyn Fn(&PrintLayout)>;

thread_local! {
    static LAYOUT: std::cell::RefCell<PrintLayout> = std::cell::RefCell::new(PrintLayout::default());
    static HEADER_PROVIDER: std::cell::RefCell<Option<HeaderProvider>> = const { std::cell::RefCell::new(None) };
    static LAYOUT_SAVER: std::cell::RefCell<Option<LayoutSaver>> = const { std::cell::RefCell::new(None) };
}

/// Mise en page courante.
pub fn layout() -> PrintLayout {
    LAYOUT.with(|l| l.borrow().clone())
}

/// Change la mise en page et la fait enregistrer.
pub fn set_layout(layout: PrintLayout) {
    LAYOUT.with(|l| *l.borrow_mut() = layout.clone());
    LAYOUT_SAVER.with(|s| {
        if let Some(save) = s.borrow().as_ref() {
            save(&layout);
        }
    });
}

/// Mise en page lue au démarrage (sans réenregistrement).
pub fn init_layout(layout: PrintLayout, saver: impl Fn(&PrintLayout) + 'static) {
    LAYOUT.with(|l| *l.borrow_mut() = layout);
    LAYOUT_SAVER.with(|s| *s.borrow_mut() = Some(Box::new(saver)));
}

/// Source de l'en-tête : la compétition affichée.
pub fn set_header_provider(provider: impl Fn() -> Option<PageHeader> + 'static) {
    HEADER_PROVIDER.with(|p| *p.borrow_mut() = Some(Box::new(provider)));
}

fn current_header() -> Option<PageHeader> {
    HEADER_PROVIDER.with(|p| p.borrow().as_ref().and_then(|f| f()))
}

/// Hauteur du bandeau et du pied de page, en points, pour une largeur donnée.
fn margins(width: f64, header: Option<&PageHeader>, layout: &PrintLayout) -> (f64, f64) {
    let top = if header.is_some() && layout.show_header { 12.0 * width / 100.0 } else { 0.0 };
    let bottom = if layout.show_footer || layout.show_page_numbers { 3.0 * width / 100.0 } else { 0.0 };
    (top, bottom)
}

/// Bandeau et pied de page d'une page (`Contest::DrawPage`). Les coordonnées
/// sont normalisées : 100 unités pour la largeur de la page.
fn draw_frame(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    header: Option<&PageHeader>,
    layout: &PrintLayout,
    page_name: &str,
    page: usize,
    total: usize,
) {
    let canvas = Canvas::new();
    let root = canvas.root();
    let k = width / 100.0;
    let f = layout.header_font as f64 / 100.0;
    let font = |size: f64| format!("Sans Bold {:.2}px", size * f * k);
    if let (Some(h), true) = (header, layout.show_header) {
        let color = if layout.header_color.is_empty() { h.color.clone() } else { layout.header_color.clone() };
        let band = canvas.rect(root, 0.0, 0.0, 100.0 * k, 10.0 * k);
        canvas.set(
            band,
            [
                Prop::StrokeColor("grey".into()),
                Prop::FillColor(if color.is_empty() { "#ffffff".into() } else { color }),
                Prop::LineWidth(0.3 * k),
            ],
        );
        let title = if layout.title.is_empty() { h.name.clone() } else { layout.title.clone() };
        let name = canvas.text(root, &title, 10.0 * k, 0.8 * k, 20.0 * k, Anchor::N);
        canvas.set(
            name,
            [
                Prop::Alignment(pango::Alignment::Center),
                Prop::FillColor(layout.header_text_color.clone()),
                Prop::Font(font(2.5)),
                Prop::Height(9.0 * k),
                Prop::Wrap(pango::WrapMode::WordChar),
            ],
        );
        let shadow = canvas.text(root, &h.subtitle, 50.5 * k, 3.5 * k, -1.0, Anchor::Center);
        canvas.set(shadow, [Prop::FillColor("black".into()), Prop::Font(font(4.0))]);
        let subtitle = canvas.text(root, &h.subtitle, 50.0 * k, 3.0 * k, -1.0, Anchor::Center);
        canvas.set(subtitle, [Prop::FillColor(layout.subtitle_color.clone()), Prop::Font(font(4.0))]);
        let page_item = canvas.text(root, page_name, 50.0 * k, 7.5 * k, -1.0, Anchor::Center);
        canvas.set(page_item, [Prop::FillColor(layout.header_text_color.clone()), Prop::Font(font(3.0))]);
        if !h.organizer.is_empty() {
            let organizer = canvas.text(root, &h.organizer, 99.0 * k, 4.0 * k, 20.0 * k, Anchor::E);
            canvas.set(
                organizer,
                [
                    Prop::FillColor(layout.header_text_color.clone()),
                    Prop::Font(font(2.5)),
                    Prop::Height(5.5 * k),
                    Prop::Wrap(pango::WrapMode::WordChar),
                    Prop::Alignment(pango::Alignment::Right),
                ],
            );
        }
        let date = canvas.text(root, &h.date, 99.0 * k, 8.0 * k, -1.0, Anchor::E);
        canvas.set(date, [Prop::FillColor(layout.header_text_color.clone()), Prop::Font(font(2.5))]);
    }
    let bottom = height - 0.3 * k;
    if layout.show_footer {
        let markup = if layout.footer_text.is_empty() {
            "BellePoule fencing software <span foreground=\"blue\" underline=\"single\">http://betton.escrime.free.fr</span>"
                .to_string()
        } else {
            gtk::glib::markup_escape_text(&layout.footer_text).to_string()
        };
        let footer = canvas.markup(root, &markup, 50.0 * k, bottom, 100.0 * k, Anchor::S);
        canvas.set(footer, [Prop::Alignment(pango::Alignment::Center), Prop::FillColor("black".into()), Prop::Font(font(1.7))]);
    }
    if layout.show_page_numbers && total > 1 {
        let number = canvas.text(root, &format!("{}/{}", page + 1, total), 100.0 * k, bottom, -1.0, Anchor::SE);
        canvas.set(number, [Prop::FillColor("#606060".into()), Prop::Font(font(1.5))]);
    }
    canvas.render(cr, None, 1.0);
}

/// Dessine une page complète : cadre (bandeau, pied) puis corps.
fn render_page(
    source: &mut dyn PageSource,
    page: usize,
    total: usize,
    cr: &cairo::Context,
    width: f64,
    height: f64,
    header: Option<&PageHeader>,
    layout: &PrintLayout,
) {
    let (top, bottom) = margins(width, header, layout);
    let page_name = source.page_name(page);
    draw_frame(cr, width, height, header, layout, &page_name, page, total);
    let _ = cr.save();
    cr.translate(0.0, top);
    cr.rectangle(0.0, 0.0, width, (height - top - bottom).max(1.0));
    cr.clip();
    source.draw(page, cr, width, (height - top - bottom).max(1.0));
    let _ = cr.restore();
}

/// Source de pages : calcule sa pagination pour un format de page puis dessine
/// chaque page. Sert à l'aperçu comme à l'impression. Les dimensions reçues
/// sont celles du corps de page (sous le bandeau, au-dessus du pied).
pub trait PageSource {
    /// Prépare la pagination pour une page de `width` × `height` points et
    /// renvoie le nombre de pages.
    fn begin(&mut self, width: f64, height: f64) -> usize;
    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64);
    /// Nom imprimé dans le bandeau (`Print::PageName`).
    fn page_name(&self, page: usize) -> String {
        let _ = page;
        String::new()
    }
}

/// Plusieurs sources imprimées l'une après l'autre (impression complète d'une
/// compétition).
pub struct Concat {
    parts: Vec<Box<dyn PageSource>>,
    counts: Vec<usize>,
}

impl Concat {
    pub fn new(parts: Vec<Box<dyn PageSource>>) -> Self {
        Concat { parts, counts: Vec::new() }
    }

    fn locate(&self, mut page: usize) -> Option<(usize, usize)> {
        for (i, count) in self.counts.iter().enumerate() {
            if page < *count {
                return Some((i, page));
            }
            page -= *count;
        }
        None
    }
}

impl PageSource for Concat {
    fn begin(&mut self, width: f64, height: f64) -> usize {
        self.counts = self.parts.iter_mut().map(|p| p.begin(width, height)).collect();
        self.counts.iter().sum()
    }

    fn draw(&mut self, page: usize, cr: &cairo::Context, width: f64, height: f64) {
        if let Some((i, p)) = self.locate(page) {
            self.parts[i].draw(p, cr, width, height);
        }
    }

    fn page_name(&self, page: usize) -> String {
        self.locate(page).map(|(i, p)| self.parts[i].page_name(p)).unwrap_or_default()
    }
}

/// Dialogue « Mise en page » ; renvoie vrai si elle a changé.
pub fn edit_layout(parent: Option<&gtk::Window>) -> bool {
    use bp_core::i18n::tr;
    let current = layout();
    let dialog = gtk::Dialog::new();
    dialog.set_title(Some(&tr("Page layout")));
    dialog.set_transient_for(parent);
    dialog.set_modal(true);
    dialog.set_resizable(false);
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    grid.set_margin_top(12);
    grid.set_margin_bottom(12);
    grid.set_margin_start(12);
    grid.set_margin_end(12);
    dialog.content_area().append(&grid);
    let mut row = 0;
    let mut add = |label: &str, widget: &gtk::Widget| {
        let l = gtk::Label::new(Some(label));
        l.set_xalign(0.0);
        grid.attach(&l, 0, row, 1, 1);
        grid.attach(widget, 1, row, 1, 1);
        row += 1;
    };
    let color_button = |value: &str, fallback: &str| {
        let b = gtk::ColorButton::new();
        let rgba = gtk::gdk::RGBA::parse(if value.is_empty() { fallback } else { value })
            .unwrap_or_else(|_| gtk::gdk::RGBA::parse(fallback).unwrap_or(gtk::gdk::RGBA::BLACK));
        b.set_rgba(&rgba);
        b
    };
    let hex = |b: &gtk::ColorButton| {
        let c = b.rgba();
        format!(
            "#{:02x}{:02x}{:02x}",
            (c.red() * 255.0).round() as u8,
            (c.green() * 255.0).round() as u8,
            (c.blue() * 255.0).round() as u8
        )
    };
    let percent = |value: u32| {
        let s = gtk::SpinButton::with_range(30.0, 300.0, 5.0);
        s.set_value(value as f64);
        s
    };

    let show_header = gtk::CheckButton::new();
    show_header.set_active(current.show_header);
    add(&tr("Header"), show_header.upcast_ref());
    let title = gtk::Entry::new();
    title.set_text(&current.title);
    title.set_placeholder_text(Some(&tr("Name of the competition")));
    title.set_width_chars(30);
    add(&tr("Title"), title.upcast_ref());
    let contest_color = gtk::CheckButton::with_label(&tr("Color of the competition"));
    contest_color.set_active(current.header_color.is_empty());
    let header_color = color_button(&current.header_color, "#e8b4b4");
    header_color.set_sensitive(!current.header_color.is_empty());
    {
        let header_color = header_color.clone();
        contest_color.connect_toggled(move |b| header_color.set_sensitive(!b.is_active()));
    }
    let color_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    color_box.append(&contest_color);
    color_box.append(&header_color);
    add(&tr("Header color"), color_box.upcast_ref());
    let header_text_color = color_button(&current.header_text_color, "#000000");
    add(&tr("Header text color"), header_text_color.upcast_ref());
    let subtitle_color = color_button(&current.subtitle_color, "#ffffff");
    add(&tr("Weapon / category color"), subtitle_color.upcast_ref());
    let header_font = percent(current.header_font);
    add(&tr("Header font size (%)"), header_font.upcast_ref());
    let body_font = percent(current.body_font);
    add(&tr("Sheet font size (%)"), body_font.upcast_ref());
    let names_color = color_button(&current.names_color, "#00008b");
    add(&tr("Fencer names color"), names_color.upcast_ref());
    let grey_color = color_button(&current.grey_color, "#bebebe");
    add(&tr("Greyed cells color"), grey_color.upcast_ref());
    let show_footer = gtk::CheckButton::new();
    show_footer.set_active(current.show_footer);
    add(&tr("Footer"), show_footer.upcast_ref());
    let footer_text = gtk::Entry::new();
    footer_text.set_text(&current.footer_text);
    footer_text.set_placeholder_text(Some("BellePoule fencing software"));
    add(&tr("Footer text"), footer_text.upcast_ref());
    let page_numbers = gtk::CheckButton::new();
    page_numbers.set_active(current.show_page_numbers);
    add(&tr("Page numbers"), page_numbers.upcast_ref());

    dialog.add_button(&tr("Default"), gtk::ResponseType::Reject);
    dialog.add_button(&tr("_Cancel"), gtk::ResponseType::Cancel);
    dialog.add_button(&tr("_OK"), gtk::ResponseType::Ok);
    let response = crate::dialogs::run_dialog(&dialog);
    let new = match response {
        gtk::ResponseType::Ok => PrintLayout {
            show_header: show_header.is_active(),
            title: title.text().trim().to_string(),
            header_color: if contest_color.is_active() { String::new() } else { hex(&header_color) },
            header_text_color: hex(&header_text_color),
            subtitle_color: hex(&subtitle_color),
            header_font: header_font.value_as_int().max(30) as u32,
            body_font: body_font.value_as_int().max(30) as u32,
            grey_color: hex(&grey_color),
            names_color: hex(&names_color),
            show_footer: show_footer.is_active(),
            footer_text: footer_text.text().trim().to_string(),
            show_page_numbers: page_numbers.is_active(),
        },
        gtk::ResponseType::Reject => PrintLayout::default(),
        _ => current.clone(),
    };
    dialog.destroy();
    if new != current {
        set_layout(new);
        true
    } else {
        false
    }
}

/// Aperçu puis impression d'un document. Renvoie vrai si l'impression a
/// bien été lancée (`make` est rappelé pour chaque usage : aperçu, impression).
pub fn run_document(
    parent: Option<&gtk::Window>,
    job_name: &str,
    make: impl Fn() -> Box<dyn PageSource> + 'static,
) -> Result<bool, gtk::glib::Error> {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let make: Rc<dyn Fn() -> Box<dyn PageSource>> = Rc::new(make);
    let header = current_header();

    // Aperçu
    let setup = gtk::PageSetup::new();
    let (paper_w, paper_h) = (setup.paper_width(gtk::Unit::Points), setup.paper_height(gtk::Unit::Points));
    let (left, top) = (setup.left_margin(gtk::Unit::Points), setup.top_margin(gtk::Unit::Points));
    let (page_w, page_h) = (setup.page_width(gtk::Unit::Points), setup.page_height(gtk::Unit::Points));
    let body_height = {
        let header = header.clone();
        move || {
            let (t, b) = margins(page_w, header.as_ref(), &layout());
            (page_h - t - b).max(1.0)
        }
    };
    let source = Rc::new(RefCell::new(make()));
    let count = Rc::new(Cell::new(source.borrow_mut().begin(page_w, body_height()).max(1)));
    let current = Rc::new(Cell::new(0_usize));

    let dialog = gtk::Dialog::new();
    dialog.set_title(Some(job_name));
    dialog.set_transient_for(parent);
    dialog.set_default_size(620, 800);
    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    {
        let source = source.clone();
        let current = current.clone();
        let count = count.clone();
        let header = header.clone();
        area.set_draw_func(move |_, cr, w, h| {
            cr.set_source_rgb(0.55, 0.55, 0.55);
            let _ = cr.paint();
            let factor = ((w as f64 - 20.0) / paper_w).min((h as f64 - 20.0) / paper_h).max(0.01);
            let _ = cr.save();
            cr.translate((w as f64 - paper_w * factor) / 2.0, (h as f64 - paper_h * factor) / 2.0);
            cr.scale(factor, factor);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.rectangle(0.0, 0.0, paper_w, paper_h);
            let _ = cr.fill();
            cr.rectangle(left, top, page_w, page_h);
            cr.clip();
            cr.translate(left, top);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            let layout = layout();
            render_page(source.borrow_mut().as_mut(), current.get(), count.get(), cr, page_w, page_h, header.as_ref(), &layout);
            let _ = cr.restore();
        });
    }
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.set_margin_top(6);
    bar.set_margin_bottom(6);
    bar.set_halign(gtk::Align::Center);
    let previous = gtk::Button::from_icon_name("go-previous-symbolic");
    let next = gtk::Button::from_icon_name("go-next-symbolic");
    let label = gtk::Label::new(None);
    let setup_button = gtk::Button::with_label(&bp_core::i18n::tr("Page layout…"));
    let update = {
        let label = label.clone();
        let previous = previous.clone();
        let next = next.clone();
        let area = area.clone();
        let current = current.clone();
        let count = count.clone();
        Rc::new(move || {
            label.set_text(&format!("{} / {}", current.get() + 1, count.get()));
            previous.set_sensitive(current.get() > 0);
            next.set_sensitive(current.get() + 1 < count.get());
            area.queue_draw();
        })
    };
    {
        let current = current.clone();
        let update = update.clone();
        previous.connect_clicked(move |_| {
            current.set(current.get().saturating_sub(1));
            update();
        });
    }
    {
        let current = current.clone();
        let count = count.clone();
        let update = update.clone();
        next.connect_clicked(move |_| {
            current.set((current.get() + 1).min(count.get() - 1));
            update();
        });
    }
    {
        let dialog_ref = dialog.downgrade();
        let source = source.clone();
        let make = make.clone();
        let count = count.clone();
        let current = current.clone();
        let update = update.clone();
        let body_height = body_height.clone();
        setup_button.connect_clicked(move |_| {
            let parent = dialog_ref.upgrade().map(|d| d.upcast::<gtk::Window>());
            if edit_layout(parent.as_ref()) {
                let mut fresh = make();
                count.set(fresh.begin(page_w, body_height()).max(1));
                *source.borrow_mut() = fresh;
                current.set(current.get().min(count.get() - 1));
                update();
            }
        });
    }
    bar.append(&previous);
    bar.append(&label);
    bar.append(&next);
    bar.append(&setup_button);
    let content = dialog.content_area();
    content.append(&bar);
    content.append(&area);
    dialog.add_button(&bp_core::i18n::tr("_Close"), gtk::ResponseType::Cancel);
    dialog.add_button(&bp_core::i18n::tr("_Print"), gtk::ResponseType::Accept);
    update();
    let response = crate::dialogs::run_dialog(&dialog);
    dialog.destroy();
    if response != gtk::ResponseType::Accept {
        return Ok(false);
    }

    // Impression
    let printing: Rc<RefCell<Option<(Box<dyn PageSource>, usize)>>> = Rc::new(RefCell::new(None));
    let operation = gtk::PrintOperation::new();
    operation.set_job_name(job_name);
    operation.set_unit(gtk::Unit::Points);
    {
        let printing = printing.clone();
        let make = make.clone();
        let header = header.clone();
        operation.connect_begin_print(move |operation, ctx| {
            let mut source = make();
            let (t, b) = margins(ctx.width(), header.as_ref(), &layout());
            let pages = source.begin(ctx.width(), (ctx.height() - t - b).max(1.0)).max(1);
            operation.set_n_pages(pages as i32);
            *printing.borrow_mut() = Some((source, pages));
        });
    }
    operation.connect_draw_page(move |_, ctx, number| {
        if let Some((source, pages)) = printing.borrow_mut().as_mut() {
            let layout = layout();
            render_page(source.as_mut(), number as usize, *pages, &ctx.cairo_context(), ctx.width(), ctx.height(), header.as_ref(), &layout);
        }
    });
    let result = operation.run(gtk::PrintOperationAction::PrintDialog, parent)?;
    Ok(result == gtk::PrintOperationResult::Apply)
}

/// Blocs cairo imprimés sans être coupés.
pub struct Tiled {
    pub title: String,
    pub subtitle: String,
    pub canvas: Canvas,
    pub groups: Vec<ItemId>,
    pub rows: usize,
    pub columns: usize,
    pages: Vec<Vec<(ItemId, f64, f64)>>,
    scale: f64,
}

impl Tiled {
    /// `rows = 0` choisit le nombre de lignes selon la hauteur disponible ;
    /// sinon la densité est imposée. `subtitle` (ou `title`) est le nom de page.
    pub fn new(title: &str, subtitle: &str, canvas: Canvas, groups: Vec<ItemId>, rows: usize, columns: usize) -> Self {
        Tiled {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            canvas,
            groups,
            rows,
            columns: columns.max(1),
            pages: Vec::new(),
            scale: 1.0,
        }
    }
}

impl PageSource for Tiled {
    fn begin(&mut self, page_width: f64, page_height: f64) -> usize {
        let columns = self.columns;
        let gap = 20.0;
        let width = self.groups.iter().map(|id| self.canvas.bounds(*id).width()).fold(1.0_f64, f64::max);
        let height = self.groups.iter().map(|id| self.canvas.bounds(*id).height()).fold(1.0_f64, f64::max);
        let factor = ((page_width - (columns - 1) as f64 * gap) / (columns as f64 * width))
            .min(page_height / (height + gap) / self.rows.max(1) as f64)
            .clamp(0.001, 1.0);
        self.scale = factor;
        let rows = if self.rows == 0 {
            (page_height / ((height + gap) * factor)).floor().max(1.0) as usize
        } else {
            self.rows
        };
        let capacity = rows * columns;
        self.pages.clear();
        for chunk in self.groups.chunks(capacity) {
            self.pages.push(
                chunk
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        (
                            *id,
                            (i % columns) as f64 * (page_width / columns as f64),
                            (i / columns) as f64 * (height + gap) * factor,
                        )
                    })
                    .collect(),
            );
        }
        if self.pages.is_empty() {
            self.pages.push(Vec::new());
        }
        self.pages.len()
    }

    fn draw(&mut self, number: usize, cr: &cairo::Context, _width: f64, _height: f64) {
        if let Some(items) = self.pages.get(number) {
            for (id, x, y) in items {
                let bounds = self.canvas.bounds(*id);
                let _ = cr.save();
                cr.translate(*x, *y);
                cr.scale(self.scale, self.scale);
                cr.translate(-bounds.x1, -bounds.y1);
                self.canvas.render_item(*id, cr);
                let _ = cr.restore();
            }
        }
    }

    fn page_name(&self, _page: usize) -> String {
        if self.subtitle.is_empty() {
            self.title.clone()
        } else {
            self.subtitle.clone()
        }
    }
}

/// Imprime des blocs cairo sans les couper (aperçu puis impression).
pub fn run_tiled(
    parent: Option<&gtk::Window>,
    title: &str,
    subtitle: &str,
    canvas: Canvas,
    groups: Vec<ItemId>,
    rows: usize,
    columns: usize,
) -> Result<(), gtk::glib::Error> {
    let (t, s) = (title.to_string(), subtitle.to_string());
    run_document(parent, title, move || {
        Box::new(Tiled::new(&t, &s, canvas.clone(), groups.clone(), rows, columns))
    })?;
    Ok(())
}

impl PageContext {
    /// Page A4 à 72 ppp.
    pub const A4: PageContext = PageContext {
        width: 595.0,
        height: 842.0,
        dpi_x: 72.0,
        dpi_y: 72.0,
    };

    pub fn new(width: f64, height: f64, dpi_x: f64, dpi_y: f64) -> Self {
        PageContext {
            width,
            height,
            dpi_x,
            dpi_y,
        }
    }

    pub fn from_gtk(ctx: &gtk::PrintContext) -> Self {
        PageContext {
            width: ctx.width(),
            height: ctx.height(),
            dpi_x: ctx.dpi_x(),
            dpi_y: ctx.dpi_y(),
        }
    }
}

/// Rend une page (cadre compris) dans un PNG au format A4 (vérifications
/// visuelles, aperçus hors interface).
pub fn export_png(
    source: &mut dyn PageSource,
    header: Option<&PageHeader>,
    page: usize,
    path: &std::path::Path,
) -> Result<(), String> {
    let (w, h) = (595.0, 842.0);
    let scale = 2.0;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, (w * scale) as i32, (h * scale) as i32)
        .map_err(|e| e.to_string())?;
    let cr = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.paint();
    cr.scale(scale, scale);
    cr.translate(18.0, 18.0);
    let (pw, ph) = (w - 36.0, h - 36.0);
    let layout = layout();
    let (t, b) = margins(pw, header, &layout);
    let total = source.begin(pw, ph - t - b).max(1);
    render_page(source, page, total, &cr, pw, ph, header, &layout);
    drop(cr);
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    surface.write_to_png(&mut file).map_err(|e| e.to_string())
}
