//! Presets: the look of a sheet under a name (paper, format, frame line, headline style and place, export format –
//! not the source and not the headline text), kept in presets.json in Application Support. `preset_json` turns the
//! window's fields into such a look, `apply_preset` sets them from one; projects keep their look the same way.

use super::*;

thread_local! {
    /// the presets by name, sorted; loaded at start, written to presets.json after every change
    pub static PRESETS: RefCell<Vec<(String, serde_json::Value)>> = const { RefCell::new(Vec::new()) };
}

pub fn presets_path() -> Option<PathBuf> {
    app_dir().map(|d| d.join("presets.json"))
}

/// Reads presets.json (a missing or broken file = no presets) and fills the menu.
pub fn load_presets() {
    let list: Vec<(String, serde_json::Value)> = presets_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Vec<serde_json::Value>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| Some((e.get("name")?.as_str()?.to_string(), e.get("values")?.clone())))
        .collect();
    PRESETS.with(|p| *p.borrow_mut() = list);
}

/// The preset menu in every window.
// shortcut: another window's chosen preset index is not moved when a preset is added or deleted elsewhere
pub fn preset_menu(_app: &AppWindow) {
    let model: Vec<SharedString> = PRESETS.with(|p| p.borrow().iter().map(|e| e.0.as_str().into()).collect());
    for a in all_windows() {
        a.set_presets(ModelRc::new(VecModel::from(model.clone())));
    }
}

/// Writes all presets atomically and refreshes the menu; `status` is the line under it when saving works.
pub fn store_presets(app: &AppWindow, status: String) {
    preset_menu(app);
    let list: Vec<serde_json::Value> = PRESETS.with(|p| p.borrow().iter().map(|(n, v)| serde_json::json!({"name": n, "values": v})).collect());
    let result = presets_path().ok_or_else(|| std::io::Error::other("no home folder")).and_then(|path| {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let part = path.with_extension("json.part");
        std::fs::write(&part, serde_json::to_vec_pretty(&list).unwrap_or_default())?;
        std::fs::rename(&part, &path)
    });
    match result {
        Ok(()) => app.set_preset_status(status.into()),
        Err(e) => {
            log::write(&format!("presets not saved: {e}"));
            app.set_preset_status(tr(["Could not save the presets: {}", "Presets ließen sich nicht sichern: {}", "Impossible d'enregistrer les préréglages : {}", "No se pudieron guardar los preajustes: {}"]).replacen("{}", &e.to_string(), 1).into());
        }
    }
}

/// Everything about the look of the sheet, as JSON: paper, picture format, line, headline style and place, crop
/// marks, export format. Not the source and not the headline text: they belong to the project. The font is kept by
/// name, so the preset works on another Mac.
pub fn preset_json(app: &AppWindow) -> serde_json::Value {
    serde_json::json!({
        "px-units": app.get_px_units(), "paper": app.get_paper(), "custom-w": app.get_custom_w().as_str(), "custom-h": app.get_custom_h().as_str(),
        "landscape": app.get_landscape(), "dpi": app.get_dpi().as_str(), "margin": app.get_margin().as_str(),
        "gap": app.get_gap().as_str(), "auto-cols": app.get_auto_cols(), "cols": app.get_cols().as_str(),
        "picture": app.get_picture(), "aspect": app.get_aspect().as_str(), "trim": app.get_trim(), "bg-color": app.get_bg_color().as_str(),
        "line-on": app.get_line_on(), "line-width": app.get_line_width().as_str(), "line-inset": app.get_line_inset().as_str(),
        "line-color": app.get_line_color().as_str(), "line-pattern": app.get_line_pattern(),
        "texts": text_styles(app), "global-angle": app.get_global_angle().as_str(),
        "mosaic-on": app.get_mosaic_on(), "mosaic-cols": app.get_mosaic_cols().as_str(), "mosaic-frames": app.get_mosaic_frames().as_str(),
        "mosaic-repeat": app.get_mosaic_repeat().as_str(), "mosaic-tint": app.get_mosaic_tint().as_str(), "mosaic-overlay": app.get_mosaic_overlay().as_str(),
        "export-format": app.get_export_format(),
    })
}

/// Style and place of a text element (its fields), without its words; the font by name.
pub fn row_style(r: &TextRow) -> serde_json::Value {
    let font = if r.font_name.is_empty() { FONTS.with(|f| f.borrow().get(r.font as usize).cloned()).unwrap_or_default() } else { r.font_name.to_string() };
    serde_json::json!({
        "font": font, "head-size": r.size.as_str(), "head-bold": r.bold, "head-italic": r.italic,
        "head-color": r.color.as_str(), "head-anchor": r.anchor, "head-dx": r.dx.as_str(), "head-dy": r.dy.as_str(),
        "head-pad": [r.pad_t.as_str(), r.pad_r.as_str(), r.pad_b.as_str(), r.pad_l.as_str()],
        "head-wrap": r.wrap, "head-lines": r.lines,
        "sh-on": r.sh_on, "sh-blend": r.sh_blend, "sh-color": r.sh_color.as_str(), "sh-opacity": r.sh_opacity.as_str(),
        "sh-angle": r.sh_angle.as_str(), "sh-global": r.sh_global, "sh-distance": r.sh_distance.as_str(),
        "sh-spread": r.sh_spread.as_str(), "sh-size": r.sh_size.as_str(), "sh-contour": r.sh_contour,
        "sh-aa": r.sh_aa, "sh-noise": r.sh_noise.as_str(), "sh-knock": r.sh_knock,
    })
}

/// A new text's fields before anything changes them (the font is the standard one once the list is loaded).
pub fn default_row() -> TextRow {
    let t = |s: &str| SharedString::from(s);
    TextRow {
        open: true,
        size: t("24"),
        color: t("#000000"),
        swatch: slint::Color::from_rgb_u8(0, 0, 0),
        anchor: 1,
        dx: t("0"),
        dy: t("94"),
        pad_t: t("24"),
        pad_r: t("24"),
        pad_b: t("24"),
        pad_l: t("24"),
        lines: 1,
        sh_blend: 2,
        sh_color: t("#000000"),
        sh_swatch: slint::Color::from_rgb_u8(0, 0, 0),
        sh_opacity: t("75"),
        sh_angle: t("120"),
        sh_global: true,
        sh_distance: t("12"),
        sh_spread: t("0"),
        sh_size: t("18"),
        sh_noise: t("0"),
        sh_knock: true,
        ..Default::default()
    }
}

/// `base` with the keys of a style set (missing or wrong keys keep the field); a font comes by name and is matched
/// to the font list by `resolve_fonts`.
pub fn style_row(v: &serde_json::Value, base: TextRow) -> TextRow {
    let mut r = base;
    let int = |k: &str, max: i64| v.get(k).and_then(|x| x.as_i64()).filter(|n| (0..=max).contains(n)).map(|n| n as i32);
    let b = |k: &str| v.get(k).and_then(|x| x.as_bool());
    let t = |k: &str| v.get(k).and_then(|x| x.as_str()).filter(|s| s.len() < 100).map(SharedString::from);
    if let Some(name) = v.get("font").and_then(|x| x.as_str()).filter(|n| !n.is_empty() && n.len() < 200) {
        r.font_name = name.into();
    }
    macro_rules! set {
        ($f:ident, $e:expr) => {
            if let Some(x) = $e {
                r.$f = x;
            }
        };
    }
    set!(size, t("head-size"));
    set!(bold, b("head-bold"));
    set!(italic, b("head-italic"));
    set!(color, t("head-color"));
    set!(anchor, int("head-anchor", 8));
    set!(dx, t("head-dx"));
    set!(dy, t("head-dy"));
    if let Some(p) = v.get("head-pad").and_then(|x| x.as_array()).filter(|p| p.len() == 4) {
        let get = |k: usize| p[k].as_str().filter(|s| s.len() < 10).map(SharedString::from);
        set!(pad_t, get(0));
        set!(pad_r, get(1));
        set!(pad_b, get(2));
        set!(pad_l, get(3));
    }
    set!(wrap, b("head-wrap"));
    set!(lines, int("head-lines", 2));
    set!(sh_on, b("sh-on"));
    set!(sh_blend, int("sh-blend", layout::Blend::ALL.len() as i64 - 1));
    set!(sh_color, t("sh-color"));
    set!(sh_opacity, t("sh-opacity"));
    set!(sh_angle, t("sh-angle"));
    set!(sh_global, b("sh-global"));
    set!(sh_distance, t("sh-distance"));
    set!(sh_spread, t("sh-spread"));
    set!(sh_size, t("sh-size"));
    set!(sh_contour, int("sh-contour", layout::Contour::ALL.len() as i64 - 1));
    set!(sh_aa, b("sh-aa"));
    set!(sh_noise, t("sh-noise"));
    set!(sh_knock, b("sh-knock"));
    r
}

/// The styles of all text elements.
pub fn text_styles(app: &AppWindow) -> Vec<serde_json::Value> {
    text_rows(app).iter().map(row_style).collect()
}

/// Keys of a text style; an old preset has them at the top level (one headline).
fn is_text_key(k: &str) -> bool {
    k == "font" || k.starts_with("head-") || k.starts_with("sh-")
}

/// Styles from a preset or project look: by place onto the texts there are (more styles add empty texts, texts
/// beyond them keep theirs); a style only changes the keys it has. Old looks hold one style at the top level.
fn apply_text_styles(app: &AppWindow, v: &serde_json::Value) {
    let styles: Vec<serde_json::Value> = match v.get("texts").and_then(|x| x.as_array()) {
        Some(a) => a.clone(),
        None => {
            let flat: serde_json::Map<String, serde_json::Value> = v.as_object().into_iter().flatten().filter(|(k, _)| is_text_key(k)).map(|(k, x)| (k.clone(), x.clone())).collect();
            if flat.is_empty() { vec![] } else { vec![flat.into()] }
        }
    };
    let mut rows = text_rows(app);
    for (k, style) in styles.iter().enumerate() {
        if k >= rows.len() {
            let base = TextRow { words: "".into(), title: "".into(), ..rows.last().cloned().unwrap_or_else(default_row) };
            rows.push(base);
        }
        rows[k] = style_row(style, rows[k].clone());
    }
    set_text_rows(app, rows);
    resolve_fonts(app);
}

/// Sets the fields from a preset. Missing or wrong keys keep the field as it is (older or newer presets).
pub fn apply_preset(app: &AppWindow, v: &serde_json::Value) {
    let int = |k: &str, max: i64, set: &dyn Fn(i32)| {
        if let Some(n) = v.get(k).and_then(|x| x.as_i64()).filter(|n| (0..=max).contains(n)) {
            set(n as i32)
        }
    };
    let boolean = |k: &str, set: &dyn Fn(bool)| v.get(k).and_then(|x| x.as_bool()).into_iter().for_each(set);
    let text = |k: &str, set: &dyn Fn(SharedString)| v.get(k).and_then(|x| x.as_str()).filter(|s| s.len() < 100).into_iter().for_each(|s| set(s.into()));
    boolean("px-units", &|b| app.set_px_units(b));
    int("paper", (layout::DIN.len() + layout::SOCIAL.len()) as i64, &|n| app.set_paper(n));
    text("custom-w", &|s| app.set_custom_w(s));
    text("custom-h", &|s| app.set_custom_h(s));
    boolean("landscape", &|b| app.set_landscape(b));
    text("dpi", &|s| app.set_dpi(s));
    text("margin", &|s| app.set_margin(s));
    text("gap", &|s| app.set_gap(s));
    boolean("auto-cols", &|b| app.set_auto_cols(b));
    text("cols", &|s| app.set_cols(s));
    // the format as typed ("2.39:1", "9:16") decides; the menu index only says Full frame or not
    int("picture", layout::PICTURE.len() as i64 + 1, &|n| app.set_picture(n));
    text("aspect", &|s| app.set_aspect(s));
    if app.get_picture() > 0 {
        picture_edited(app);
    }
    show_picture(app);
    boolean("trim", &|b| app.set_trim(b));
    text("bg-color", &|s| app.set_bg_color(s));
    boolean("line-on", &|b| app.set_line_on(b));
    text("line-width", &|s| app.set_line_width(s));
    text("line-inset", &|s| app.set_line_inset(s));
    text("line-color", &|s| app.set_line_color(s));
    int("line-pattern", layout::Pattern::ALL.len() as i64 - 1, &|n| app.set_line_pattern(n));
    text("global-angle", &|s| app.set_global_angle(s));
    boolean("mosaic-on", &|b| app.set_mosaic_on(b));
    text("mosaic-cols", &|s| app.set_mosaic_cols(s));
    text("mosaic-frames", &|s| app.set_mosaic_frames(s));
    text("mosaic-repeat", &|s| app.set_mosaic_repeat(s));
    text("mosaic-tint", &|s| app.set_mosaic_tint(s));
    text("mosaic-overlay", &|s| app.set_mosaic_overlay(s));
    apply_text_styles(app, v);
    int("export-format", 3, &|n| app.set_export_format(n));
    match_slots(app);
    // an A or social media size from the list fills the fields
    if app.get_paper() > 0 {
        show_paper(app);
    }
}
