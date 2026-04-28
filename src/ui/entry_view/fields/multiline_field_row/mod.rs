use gtk::{gio, glib, prelude::*, subclass::prelude::*};

use crate::helpers::clipboard;
use crate::pass::model::EntryField;

use super::super::EntryView;
use super::EntryFieldRow;

mod imp;

glib::wrapper! {
    pub struct MultilineFieldRow(ObjectSubclass<imp::MultilineFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl MultilineFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        imp.title_entry.connect_changed(glib::clone!(
            #[weak]
            entry_view,
            move |_| {
                entry_view.mark_changed();
            }
        ));

        let buffer = imp.value_text_view.buffer();
        buffer.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            #[weak]
            entry_view,
            move |_| {
                this.sync_value_view();
                entry_view.mark_changed();
            }
        ));

        imp.copy_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            move |_| {
                clipboard::copy_secret(&this.value());
            }
        ));

        imp.delete_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            #[weak]
            entry_view,
            move |_| {
                if let Some(list) = this.parent() {
                    if let Ok(container) = list.downcast::<gtk::Box>() {
                        container.remove(&this);
                        entry_view.mark_changed();
                    }
                }
            }
        ));

        if settings_has_key(MARKDOWN_SETTING_KEY) {
            let settings = gio::Settings::new(crate::APP_ID);
            settings.connect_changed(
                Some(MARKDOWN_SETTING_KEY),
                glib::clone!(
                    #[weak(rename_to = this)]
                    obj,
                    move |_, _| {
                        this.sync_value_view();
                    }
                ),
            );
            let _ = imp.settings.set(settings);
        }

        obj
    }

    pub fn new(entry_view: &EntryView, key: &str, value: &str) -> Self {
        let obj = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_value(value);
        obj
    }

    pub fn from_entry_field(entry_view: &EntryView, key: &str, field: &EntryField) -> Option<Self> {
        match field {
            EntryField::Multiline(value) => Some(Self::new(entry_view, key, value)),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.key().trim().is_empty() && self.value().trim().is_empty()
    }

    pub fn key(&self) -> String {
        self.imp().title_entry.text().into()
    }

    pub fn set_key(&self, key: &str) {
        let title_entry = &self.imp().title_entry;
        title_entry.set_text(key);
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        imp.drag_handle.set_visible(editable);
        Self::set_entry_editable_mode(&imp.title_entry, editable);
        imp.value_view_box.set_visible(!editable);
        imp.value_scrolled_window.set_visible(editable);
        imp.value_text_view.set_editable(editable);
        imp.value_text_view.set_can_focus(editable);
        imp.value_text_view.set_cursor_visible(editable);
        imp.delete_button.set_visible(editable);
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }

    pub fn value(&self) -> String {
        let buffer = self.imp().value_text_view.buffer();
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        buffer.text(&start, &end, true).to_string()
    }

    pub fn set_value(&self, value: &str) {
        self.imp().value_text_view.buffer().set_text(value);
        self.sync_value_view();
    }

    fn sync_value_view(&self) {
        let value_view_box = &self.imp().value_view_box;
        let value = self.value();
        clear_box(value_view_box);

        if self.markdown_enabled() {
            render_markdown_widgets(value_view_box, &value);
        } else {
            value_view_box.append(&plain_text_label(&value));
        }
    }

    fn set_entry_editable_mode(entry: &gtk::Entry, editable: bool) {
        entry.set_editable(editable);
        entry.set_can_focus(editable);
        entry.set_has_frame(editable);
    }

    fn markdown_enabled(&self) -> bool {
        self.imp()
            .settings
            .get()
            .is_some_and(|settings| settings.boolean(MARKDOWN_SETTING_KEY))
    }
}

const MARKDOWN_SETTING_KEY: &str = "interpret-multiline-as-markdown";

#[derive(Debug, PartialEq, Eq)]
enum MarkdownBlock {
    Paragraph(String),
    Heading { level: usize, text: String },
    Quote(String),
    UnorderedListItem(String),
    OrderedListItem { marker: String, text: String },
    Code(String),
}

#[derive(Debug, PartialEq, Eq)]
enum InlineSegment<'a> {
    Text(&'a str),
    Styled { text: &'a str, emphasis: Emphasis },
    Code(&'a str),
    Link { label: &'a str, uri: &'a str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Emphasis {
    Normal,
    Italic,
    Bold,
    BoldItalic,
}

#[derive(Clone, Copy)]
enum InlineStyle {
    Normal,
    Heading(usize),
    Quote,
}

fn render_markdown_widgets(container: &gtk::Box, value: &str) {
    for block in markdown_blocks(value) {
        match block {
            MarkdownBlock::Paragraph(text) => {
                container.append(&inline_flow(&text, InlineStyle::Normal, None));
            }
            MarkdownBlock::Heading { level, text } => {
                container.append(&inline_flow(&text, InlineStyle::Heading(level), None));
            }
            MarkdownBlock::Quote(text) => {
                container.append(&inline_flow(&text, InlineStyle::Quote, None));
            }
            MarkdownBlock::UnorderedListItem(text) => {
                container.append(&inline_flow(&text, InlineStyle::Normal, Some("•")));
            }
            MarkdownBlock::OrderedListItem { marker, text } => {
                container.append(&inline_flow(&text, InlineStyle::Normal, Some(&marker)));
            }
            MarkdownBlock::Code(code) => container.append(&code_block_frame(&code)),
        }
    }
}

fn markdown_blocks(value: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut code_block = String::new();
    let mut in_code_block = false;

    for line in value.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") {
            if in_code_block {
                blocks.push(MarkdownBlock::Code(std::mem::take(&mut code_block)));
                in_code_block = false;
            } else {
                flush_paragraph(&mut blocks, &mut paragraph);
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            if !code_block.is_empty() {
                code_block.push('\n');
            }
            code_block.push_str(line);
        } else {
            if line.trim().is_empty() {
                flush_paragraph(&mut blocks, &mut paragraph);
                continue;
            }

            if let Some((level, heading)) = parse_heading(trimmed) {
                flush_paragraph(&mut blocks, &mut paragraph);
                blocks.push(MarkdownBlock::Heading {
                    level,
                    text: heading.trim().to_string(),
                });
                continue;
            }

            if let Some(quote) = trimmed.strip_prefix("> ") {
                flush_paragraph(&mut blocks, &mut paragraph);
                blocks.push(MarkdownBlock::Quote(quote.trim().to_string()));
                continue;
            }

            if let Some(item) = parse_unordered_list_item(trimmed) {
                flush_paragraph(&mut blocks, &mut paragraph);
                blocks.push(MarkdownBlock::UnorderedListItem(item.trim().to_string()));
                continue;
            }

            if let Some((number, item)) = parse_ordered_list_item(trimmed) {
                flush_paragraph(&mut blocks, &mut paragraph);
                blocks.push(MarkdownBlock::OrderedListItem {
                    marker: format!("{number}."),
                    text: item.trim().to_string(),
                });
                continue;
            }

            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line.trim());
        }
    }

    if in_code_block {
        blocks.push(MarkdownBlock::Code(code_block));
    }
    flush_paragraph(&mut blocks, &mut paragraph);

    blocks
}

fn flush_paragraph(blocks: &mut Vec<MarkdownBlock>, paragraph: &mut String) {
    if paragraph.is_empty() {
        return;
    }

    blocks.push(MarkdownBlock::Paragraph(std::mem::take(paragraph)));
}

fn inline_flow(text: &str, style: InlineStyle, marker: Option<&str>) -> gtk::FlowBox {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(false);
    flow.set_column_spacing(4);
    flow.set_row_spacing(2);
    flow.set_max_children_per_line(1000);

    if let Some(marker) = marker {
        flow.insert(&text_label(marker, Emphasis::Normal, style), -1);
    }

    for segment in parse_inline_segments(text) {
        match segment {
            InlineSegment::Text(text) => append_text_segment(&flow, text, Emphasis::Normal, style),
            InlineSegment::Styled { text, emphasis } => {
                append_text_segment(&flow, text, emphasis, style)
            }
            InlineSegment::Code(text) => flow.insert(&inline_code_frame(text), -1),
            InlineSegment::Link { label, uri } => flow.insert(&link_label(label, uri, style), -1),
        }
    }

    flow
}

fn append_text_segment(flow: &gtk::FlowBox, text: &str, emphasis: Emphasis, style: InlineStyle) {
    if let Some(text) = display_text_segment(text) {
        flow.insert(&text_label(text, emphasis, style), -1);
    }
}

fn display_text_segment(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty()).then_some(text)
}

fn text_label(text: &str, emphasis: Emphasis, style: InlineStyle) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_selectable(true);
    if let InlineStyle::Heading(level) = style {
        label.add_css_class(&heading_css_class(level));
    }
    label.set_use_markup(true);
    label.set_markup(&styled_text_markup(text, emphasis, style));
    label
}

fn styled_text_markup(text: &str, emphasis: Emphasis, style: InlineStyle) -> String {
    let escaped = escape_markup(text);
    let escaped = apply_emphasis_markup(&escaped, emphasis);

    style_markup(escaped, style)
}

fn style_markup(markup: String, style: InlineStyle) -> String {
    match style {
        InlineStyle::Normal => markup,
        InlineStyle::Heading(_) => format!("<b>{markup}</b>"),
        InlineStyle::Quote => format!("<span style=\"italic\">{markup}</span>"),
    }
}

fn apply_emphasis_markup(escaped: &str, emphasis: Emphasis) -> String {
    match emphasis {
        Emphasis::Normal => escaped.to_string(),
        Emphasis::Italic => format!("<i>{escaped}</i>"),
        Emphasis::Bold => format!("<b>{escaped}</b>"),
        Emphasis::BoldItalic => format!("<b><i>{escaped}</i></b>"),
    }
}

fn inline_code_frame(text: &str) -> gtk::Frame {
    framed_widget(&code_flow(text), false, 2, 6)
}

fn code_block_frame(text: &str) -> gtk::Frame {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 0);
    block.set_hexpand(true);

    for line in text.lines() {
        block.append(&code_flow(line));
    }

    if text.is_empty() {
        block.append(&code_flow(""));
    }

    framed_widget(&block, true, 8, 8)
}

fn framed_widget<W>(
    child: &W,
    hexpand: bool,
    vertical_margin: i32,
    horizontal_margin: i32,
) -> gtk::Frame
where
    W: IsA<gtk::Widget>,
{
    child.set_margin_top(vertical_margin);
    child.set_margin_bottom(vertical_margin);
    child.set_margin_start(horizontal_margin);
    child.set_margin_end(horizontal_margin);

    let frame = gtk::Frame::new(None);
    frame.set_hexpand(hexpand);
    frame.set_child(Some(child));
    frame
}

fn code_flow(text: &str) -> gtk::FlowBox {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(false);
    flow.set_column_spacing(0);
    flow.set_row_spacing(0);
    flow.set_max_children_per_line(1000);
    flow.insert(&code_label(text), -1);

    flow
}

fn code_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_selectable(true);
    label.set_use_markup(true);
    label.set_markup(&format!(
        "<span font_family=\"monospace\">{}</span>",
        escape_markup(text)
    ));
    label
}

fn link_label(label: &str, uri: &str, style: InlineStyle) -> gtk::Label {
    let link = format!(
        "<a href=\"{}\">{}</a>",
        escape_markup(uri),
        escape_markup(label)
    );

    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_selectable(true);
    if let InlineStyle::Heading(level) = style {
        label.add_css_class(&heading_css_class(level));
    }
    label.set_use_markup(true);
    label.set_markup(&style_markup(link, style));
    label
}

fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let level = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if (1..=6).contains(&level) && line[level..].starts_with(' ') {
        Some((level, &line[level + 1..]))
    } else {
        None
    }
}

/// custom fields key is already title-2, so it must be level + 1
fn heading_css_class(level: usize) -> String {
    format!("title-{}", level + 1)
}

fn parse_unordered_list_item(line: &str) -> Option<&str> {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
}

fn parse_ordered_list_item(line: &str) -> Option<(&str, &str)> {
    let (number, rest) = line.split_once(". ")?;

    if !number.is_empty() && number.chars().all(|character| character.is_ascii_digit()) {
        Some((number, rest))
    } else {
        None
    }
}

fn parse_inline_segments(value: &str) -> Vec<InlineSegment<'_>> {
    let mut segments = Vec::new();
    let mut index = 0;

    while index < value.len() {
        let rest = &value[index..];

        if let Some((token_len, inner, consumed, emphasis)) = parse_emphasis(
            rest,
            &[("___", Emphasis::BoldItalic), ("***", Emphasis::BoldItalic)],
        ) {
            segments.push(InlineSegment::Styled {
                text: inner,
                emphasis,
            });
            index += token_len + consumed + token_len;
            continue;
        }

        if let Some((token_len, inner, consumed, emphasis)) =
            parse_emphasis(rest, &[("__", Emphasis::Bold), ("**", Emphasis::Bold)])
        {
            segments.push(InlineSegment::Styled {
                text: inner,
                emphasis,
            });
            index += token_len + consumed + token_len;
            continue;
        }

        if let Some((token_len, inner, consumed, emphasis)) =
            parse_emphasis(rest, &[("_", Emphasis::Italic), ("*", Emphasis::Italic)])
        {
            segments.push(InlineSegment::Styled {
                text: inner,
                emphasis,
            });
            index += token_len + consumed + token_len;
            continue;
        }

        if let Some((token_len, inner, consumed)) = parse_wrapped(rest, &["`"]) {
            segments.push(InlineSegment::Code(inner));
            index += token_len + consumed + token_len;
            continue;
        }

        if let Some((label, uri, consumed)) = parse_link(rest) {
            segments.push(InlineSegment::Link { label, uri });
            index += consumed;
            continue;
        }

        let next_special = rest
            .char_indices()
            .skip(1)
            .find_map(|(offset, character)| {
                matches!(character, '*' | '_' | '`' | '[').then_some(offset)
            })
            .unwrap_or(rest.len());
        segments.push(InlineSegment::Text(&rest[..next_special]));
        index += next_special;
    }

    segments
}

fn parse_emphasis<'a>(
    value: &'a str,
    tokens: &[(&str, Emphasis)],
) -> Option<(usize, &'a str, usize, Emphasis)> {
    for (token, emphasis) in tokens {
        let inner_start = token.len();
        if !value.starts_with(token) {
            continue;
        }

        let inner_end = value[inner_start..].find(token)? + inner_start;
        if inner_end == inner_start {
            continue;
        }

        return Some((
            token.len(),
            &value[inner_start..inner_end],
            inner_end - inner_start,
            *emphasis,
        ));
    }

    None
}

fn parse_wrapped<'a>(value: &'a str, tokens: &[&str]) -> Option<(usize, &'a str, usize)> {
    for token in tokens {
        let inner_start = token.len();
        if !value.starts_with(token) {
            continue;
        }

        let inner_end = value[inner_start..].find(token)? + inner_start;
        if inner_end == inner_start {
            continue;
        }

        return Some((
            token.len(),
            &value[inner_start..inner_end],
            inner_end - inner_start,
        ));
    }

    None
}

fn parse_link(value: &str) -> Option<(&str, &str, usize)> {
    let label_end = value.strip_prefix('[')?.find("](")? + 1;
    let uri_start = label_end + 2;
    let uri_end = value[uri_start..].find(')')? + uri_start;
    let label = &value[1..label_end];
    let uri = &value[uri_start..uri_end];

    if label.is_empty() || uri.is_empty() {
        return None;
    }

    Some((label, uri, uri_end + 1))
}

fn escape_markup(value: &str) -> String {
    glib::markup_escape_text(value).to_string()
}

fn plain_text_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn settings_has_key(key: &str) -> bool {
    gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup(crate::APP_ID, true))
        .is_some_and(|schema| schema.has_key(key))
}

impl EntryFieldRow for MultilineFieldRow {
    fn key(&self) -> String {
        self.key().trim().to_string()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::Multiline(self.value().trim().to_string())
    }

    fn set_entry_field(&self, field: &EntryField) {
        if let EntryField::Multiline(value) = field {
            self.set_value(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_marker_emphasis() {
        assert_eq!(
            parse_inline_segments("*italic* _also italic_ __bold__ ___both___"),
            vec![
                InlineSegment::Styled {
                    text: "italic",
                    emphasis: Emphasis::Italic,
                },
                InlineSegment::Text(" "),
                InlineSegment::Styled {
                    text: "also italic",
                    emphasis: Emphasis::Italic,
                },
                InlineSegment::Text(" "),
                InlineSegment::Styled {
                    text: "bold",
                    emphasis: Emphasis::Bold,
                },
                InlineSegment::Text(" "),
                InlineSegment::Styled {
                    text: "both",
                    emphasis: Emphasis::BoldItalic,
                },
            ]
        );
    }

    #[test]
    fn parses_inline_code_and_links_as_widget_segments() {
        assert_eq!(
            parse_inline_segments("open `code` then [site](https://example.invalid)"),
            vec![
                InlineSegment::Text("open "),
                InlineSegment::Code("code"),
                InlineSegment::Text(" then "),
                InlineSegment::Link {
                    label: "site",
                    uri: "https://example.invalid",
                },
            ]
        );
    }

    #[test]
    fn keeps_same_style_words_in_one_text_label() {
        assert_eq!(display_text_segment("This is "), Some("This is"));
        assert_eq!(display_text_segment(" new example"), Some("new example"));
        assert_eq!(display_text_segment(" "), None);
    }

    #[test]
    fn parses_fenced_code_as_block() {
        assert_eq!(
            markdown_blocks("before\n```\na < b\nc > d\n```\nafter"),
            vec![
                MarkdownBlock::Paragraph("before".into()),
                MarkdownBlock::Code("a < b\nc > d".into()),
                MarkdownBlock::Paragraph("after".into()),
            ]
        );
    }

    #[test]
    fn merges_soft_newlines_into_paragraphs() {
        assert_eq!(
            markdown_blocks("hello\nworld\n\nnext paragraph"),
            vec![
                MarkdownBlock::Paragraph("hello world".into()),
                MarkdownBlock::Paragraph("next paragraph".into()),
            ]
        );
    }

    #[test]
    fn parses_structural_markdown_blocks() {
        assert_eq!(
            markdown_blocks("# Title\n- item\n2. next"),
            vec![
                MarkdownBlock::Heading {
                    level: 1,
                    text: "Title".into(),
                },
                MarkdownBlock::UnorderedListItem("item".into()),
                MarkdownBlock::OrderedListItem {
                    marker: "2.".into(),
                    text: "next".into(),
                },
            ]
        );
    }

    #[test]
    fn maps_heading_levels_to_next_title_class() {
        assert_eq!(heading_css_class(1), "title-2");
        assert_eq!(heading_css_class(2), "title-3");
    }

    #[test]
    fn escapes_markup_in_text_labels() {
        assert_eq!(
            styled_text_markup("<unsafe>", Emphasis::Bold, InlineStyle::Normal),
            "<b>&lt;unsafe&gt;</b>"
        );
    }
}
