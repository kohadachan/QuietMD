use crate::markdown::{BlockKind, Document, StyleSpan};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory,
    ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_HIT_TEST_METRICS, DWRITE_LINE_METRICS, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_EMERGENCY_BREAK,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::core::{PCWSTR, Result, w};
use windows_numerics::Vector2;

const PAGE_SIDE_PADDING: f32 = 32.0;
const PAGE_NARROW_PADDING: f32 = 16.0;
const PAGE_TOP_PADDING: f32 = 28.0;
const CODE_HORIZONTAL_PADDING: f32 = 12.0;
const CODE_VERTICAL_PADDING: f32 = 12.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontChoice {
    BizUdGothic,
    #[default]
    SegoeUi,
    Arial,
    Georgia,
    Consolas,
    YuGothicUi,
    Meiryo,
}

impl FontChoice {
    const fn family_name(self) -> &'static str {
        match self {
            Self::BizUdGothic => "BIZ UDGothic",
            Self::SegoeUi => "Segoe UI",
            Self::Arial => "Arial",
            Self::Georgia => "Georgia",
            Self::Consolas => "Consolas",
            Self::YuGothicUi => "Yu Gothic UI",
            Self::Meiryo => "Meiryo",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineSpacingChoice {
    Compact,
    #[default]
    Standard,
    Relaxed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewSettings {
    pub font: FontChoice,
    pub font_size: u8,
    pub line_spacing: LineSpacingChoice,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            font: FontChoice::SegoeUi,
            font_size: 15,
            line_spacing: LineSpacingChoice::Standard,
        }
    }
}

pub struct Renderer {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    body_format: IDWriteTextFormat,
    settings: ViewSettings,
    dpi: f32,
    target: Option<TargetResources>,
}

struct TargetResources {
    render_target: ID2D1HwndRenderTarget,
    text_brush: ID2D1SolidColorBrush,
    muted_brush: ID2D1SolidColorBrush,
    accent_brush: ID2D1SolidColorBrush,
    panel_brush: ID2D1SolidColorBrush,
    line_brush: ID2D1SolidColorBrush,
    selection_brush: ID2D1SolidColorBrush,
}

pub struct LayoutDocument {
    pub blocks: Vec<LayoutBlock>,
    pub total_height: f32,
    plain_text: Vec<u16>,
}

pub struct LayoutBlock {
    kind: BlockKind,
    layout: Option<IDWriteTextLayout>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text_start: u32,
    text_len: u32,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let settings = ViewSettings::default();
            let body_format = create_body_format(&dwrite, settings)?;

            Ok(Self {
                d2d,
                dwrite,
                body_format,
                settings,
                dpi: 96.0,
                target: None,
            })
        }
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        self.dpi = if dpi == 0 { 96.0 } else { dpi as f32 };
        if let Some(resources) = self.target.as_ref() {
            unsafe {
                resources.render_target.SetDpi(self.dpi, self.dpi);
            }
        }
    }

    pub const fn settings(&self) -> ViewSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: ViewSettings) -> Result<()> {
        self.body_format = create_body_format(&self.dwrite, settings)?;
        self.settings = settings;
        Ok(())
    }

    pub fn layout(&self, document: &Document, viewport_width: f32) -> Result<LayoutDocument> {
        let (page_left, page_width) = page_geometry(viewport_width);
        let mut y = PAGE_TOP_PADDING;
        let mut blocks = Vec::with_capacity(document.blocks.len());
        let mut plain_text = Vec::new();

        for block in &document.blocks {
            if !blocks.is_empty() {
                plain_text.push('\n' as u16);
            }
            let text_start = plain_text.len() as u32;
            if matches!(block.kind, BlockKind::Rule) {
                blocks.push(LayoutBlock {
                    kind: block.kind.clone(),
                    layout: None,
                    x: page_left,
                    y: y + 10.0,
                    width: page_width,
                    height: 22.0,
                    text_start,
                    text_len: 0,
                });
                y += 30.0;
                continue;
            }

            let (indent, top_margin, bottom_margin) = block_spacing(&block.kind);
            if !blocks.is_empty() {
                y += top_margin;
            }
            let content_width = if matches!(block.kind, BlockKind::Code) {
                (page_width - CODE_HORIZONTAL_PADDING * 2.0).max(120.0)
            } else {
                (page_width - indent).max(120.0)
            };
            let visible_text = match &block.kind {
                BlockKind::Image { source, alt } => {
                    let label = if alt.trim().is_empty() {
                        "image"
                    } else {
                        alt.trim()
                    };
                    if is_remote_source(source) {
                        format!("[remote image omitted] {label}")
                    } else {
                        format!("[image] {label}\n{source}")
                    }
                }
                _ => block.text.clone(),
            };

            let wide: Vec<u16> = visible_text.encode_utf16().collect();
            plain_text.extend_from_slice(&wide);
            let text_layout = unsafe {
                self.dwrite
                    .CreateTextLayout(&wide, &self.body_format, content_width, 100_000.0)?
            };
            unsafe {
                text_layout.SetWordWrapping(DWRITE_WORD_WRAPPING_EMERGENCY_BREAK)?;
            }
            let block_font_size = apply_block_style(
                &text_layout,
                &block.kind,
                wide.len() as u32,
                self.settings.font_size as f32,
            )?;
            apply_line_spacing(&text_layout, self.settings.line_spacing, block_font_size)?;
            apply_inline_styles(&text_layout, &block.spans)?;
            apply_code_language_style(&text_layout, &block.kind, &block.spans, block_font_size)?;

            let mut metrics = DWRITE_TEXT_METRICS::default();
            unsafe { text_layout.GetMetrics(&mut metrics)? };
            let height = metrics.height.max(match block.kind {
                BlockKind::Image { .. } => 48.0,
                _ => 18.0,
            });

            blocks.push(LayoutBlock {
                kind: block.kind.clone(),
                layout: Some(text_layout),
                x: page_left + indent,
                y,
                width: content_width,
                height,
                text_start,
                text_len: wide.len() as u32,
            });
            y += height + bottom_margin;
        }

        Ok(LayoutDocument {
            blocks,
            total_height: y + PAGE_TOP_PADDING,
            plain_text,
        })
    }

    pub fn hit_test(&self, document: &LayoutDocument, x: f32, y: f32) -> Result<u32> {
        let text_blocks = document
            .blocks
            .iter()
            .filter(|block| block.layout.is_some() && block.text_len > 0)
            .collect::<Vec<_>>();
        let Some(first) = text_blocks.first() else {
            return Ok(0);
        };
        if y <= first.y {
            return Ok(first.text_start);
        }

        for block in text_blocks {
            if y < block.y {
                return Ok(block.text_start);
            }
            if y <= block.y + block.height {
                let Some(text_layout) = block.layout.as_ref() else {
                    continue;
                };
                let mut trailing = windows::core::BOOL::default();
                let mut inside = windows::core::BOOL::default();
                let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                unsafe {
                    text_layout.HitTestPoint(
                        x - block.x,
                        y - block.y,
                        &mut trailing,
                        &mut inside,
                        &mut metrics,
                    )?;
                }
                let trailing_length = if trailing.as_bool() {
                    metrics.length
                } else {
                    0
                };
                let local = (metrics.textPosition + trailing_length).min(block.text_len);
                return Ok(block.text_start + local);
            }
        }
        Ok(document.text_len())
    }

    pub fn line_range_at(&self, document: &LayoutDocument, y: f32) -> Result<Option<(u32, u32)>> {
        for block in &document.blocks {
            if y < block.y || y > block.y + block.height {
                continue;
            }
            let Some(text_layout) = block.layout.as_ref() else {
                return Ok(None);
            };
            let lines = line_metrics(text_layout)?;
            let mut line_top = block.y;
            let mut local_start = 0u32;
            for (index, line) in lines.iter().enumerate() {
                let is_last = index + 1 == lines.len();
                if y <= line_top + line.height || is_last {
                    let visible_length = line.length.saturating_sub(line.newlineLength);
                    let start = block.text_start + local_start;
                    return Ok(Some((start, start + visible_length)));
                }
                line_top += line.height;
                local_start += line.length;
            }
            return Ok(None);
        }
        Ok(None)
    }

    pub fn paint(
        &mut self,
        hwnd: HWND,
        layout: &LayoutDocument,
        viewport_width_px: u32,
        viewport_height_px: u32,
        viewport_height_dip: f32,
        scroll_y: f32,
        selection: Option<(u32, u32)>,
    ) -> Result<()> {
        self.ensure_target(hwnd, viewport_width_px, viewport_height_px)?;
        let Some(resources) = self.target.as_ref() else {
            return Ok(());
        };
        let target = &resources.render_target;

        unsafe {
            target.BeginDraw();
            target.Clear(Some(&color(0.975, 0.978, 0.982, 1.0)));

            for block in &layout.blocks {
                let draw_y = block.y - scroll_y;
                if draw_y + block.height < -40.0 || draw_y > viewport_height_dip + 40.0 {
                    continue;
                }

                match block.kind {
                    BlockKind::Rule => target.DrawLine(
                        Vector2 {
                            X: block.x,
                            Y: draw_y,
                        },
                        Vector2 {
                            X: block.x + block.width,
                            Y: draw_y,
                        },
                        &resources.line_brush,
                        1.0,
                        None,
                    ),
                    BlockKind::Code => target.FillRectangle(
                        &D2D_RECT_F {
                            left: block.x - CODE_HORIZONTAL_PADDING,
                            top: draw_y - CODE_VERTICAL_PADDING,
                            right: block.x + block.width + CODE_HORIZONTAL_PADDING,
                            bottom: draw_y + block.height + CODE_VERTICAL_PADDING,
                        },
                        &resources.panel_brush,
                    ),
                    BlockKind::TableRow { .. } => target.FillRectangle(
                        &D2D_RECT_F {
                            left: block.x - 12.0,
                            top: draw_y - 1.0,
                            right: block.x + block.width + 12.0,
                            bottom: draw_y + block.height + 1.0,
                        },
                        &resources.panel_brush,
                    ),
                    BlockKind::Quote => target.FillRectangle(
                        &D2D_RECT_F {
                            left: block.x - 18.0,
                            top: draw_y,
                            right: block.x - 14.0,
                            bottom: draw_y + block.height,
                        },
                        &resources.accent_brush,
                    ),
                    BlockKind::Image { .. } => target.DrawRectangle(
                        &D2D_RECT_F {
                            left: block.x - 10.0,
                            top: draw_y - 7.0,
                            right: block.x + block.width + 10.0,
                            bottom: draw_y + block.height + 7.0,
                        },
                        &resources.line_brush,
                        1.0,
                        None,
                    ),
                    _ => {}
                }

                if let Some(text_layout) = &block.layout {
                    if let Some(selection) = selection {
                        draw_selection(
                            target,
                            &resources.selection_brush,
                            block,
                            text_layout,
                            draw_y,
                            selection,
                        )?;
                    }
                    let brush = match block.kind {
                        BlockKind::Image { .. } => &resources.muted_brush,
                        _ => &resources.text_brush,
                    };
                    target.DrawTextLayout(
                        Vector2 {
                            X: block.x,
                            Y: draw_y,
                        },
                        text_layout,
                        brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }
            }

            if let Err(error) = target.EndDraw(None, None) {
                self.target = None;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if let Some(resources) = self.target.as_ref() {
            unsafe {
                resources.render_target.Resize(&D2D_SIZE_U {
                    width: width.max(1),
                    height: height.max(1),
                })?;
            }
        }
        Ok(())
    }

    fn ensure_target(&mut self, hwnd: HWND, width: u32, height: u32) -> Result<()> {
        if self.target.is_some() {
            return Ok(());
        }

        let render_properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_UNKNOWN,
                alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
            },
            dpiX: self.dpi,
            dpiY: self.dpi,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: Default::default(),
        };
        let hwnd_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: width.max(1),
                height: height.max(1),
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };

        unsafe {
            let render_target = self
                .d2d
                .CreateHwndRenderTarget(&render_properties, &hwnd_properties)?;
            let text_brush =
                render_target.CreateSolidColorBrush(&color(0.105, 0.125, 0.155, 1.0), None)?;
            let muted_brush =
                render_target.CreateSolidColorBrush(&color(0.38, 0.42, 0.48, 1.0), None)?;
            let accent_brush =
                render_target.CreateSolidColorBrush(&color(0.12, 0.43, 0.78, 1.0), None)?;
            let panel_brush =
                render_target.CreateSolidColorBrush(&color(0.925, 0.936, 0.950, 1.0), None)?;
            let line_brush =
                render_target.CreateSolidColorBrush(&color(0.76, 0.79, 0.83, 1.0), None)?;
            let selection_brush =
                render_target.CreateSolidColorBrush(&color(0.58, 0.77, 1.0, 0.65), None)?;
            self.target = Some(TargetResources {
                render_target,
                text_brush,
                muted_brush,
                accent_brush,
                panel_brush,
                line_brush,
                selection_brush,
            });
        }
        Ok(())
    }
}

impl LayoutDocument {
    pub fn text_len(&self) -> u32 {
        self.plain_text.len() as u32
    }

    pub fn selected_text(&self, anchor: u32, active: u32) -> Vec<u16> {
        let (start, end) = normalized_range(anchor, active, self.text_len());
        self.plain_text[start as usize..end as usize].to_vec()
    }
}

fn draw_selection(
    target: &ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    block: &LayoutBlock,
    text_layout: &IDWriteTextLayout,
    draw_y: f32,
    selection: (u32, u32),
) -> Result<()> {
    let (selection_start, selection_end) = normalized_range(selection.0, selection.1, u32::MAX);
    let block_end = block.text_start + block.text_len;
    let start = selection_start.max(block.text_start);
    let end = selection_end.min(block_end);
    if start >= end {
        return Ok(());
    }

    let local_start = start - block.text_start;
    let local_length = end - start;
    let initial_capacity = (local_length as usize).clamp(1, 16);
    let mut metrics = vec![DWRITE_HIT_TEST_METRICS::default(); initial_capacity];
    let mut count = 0u32;
    let first_result = unsafe {
        text_layout.HitTestTextRange(
            local_start,
            local_length,
            block.x,
            draw_y,
            Some(&mut metrics),
            &mut count,
        )
    };
    if let Err(error) = first_result {
        if count as usize <= metrics.len() {
            return Err(error);
        }
        metrics.resize(count as usize, DWRITE_HIT_TEST_METRICS::default());
        unsafe {
            text_layout.HitTestTextRange(
                local_start,
                local_length,
                block.x,
                draw_y,
                Some(&mut metrics),
                &mut count,
            )?;
        }
    }
    unsafe {
        for metric in metrics.into_iter().take(count as usize) {
            target.FillRectangle(
                &D2D_RECT_F {
                    left: metric.left,
                    top: metric.top,
                    right: metric.left + metric.width,
                    bottom: metric.top + metric.height,
                },
                brush,
            );
        }
    }
    Ok(())
}

fn line_metrics(layout: &IDWriteTextLayout) -> Result<Vec<DWRITE_LINE_METRICS>> {
    let mut count = 0u32;
    let first_result = unsafe { layout.GetLineMetrics(None, &mut count) };
    if count == 0 {
        first_result?;
        return Ok(Vec::new());
    }

    let mut metrics = vec![DWRITE_LINE_METRICS::default(); count as usize];
    unsafe {
        layout.GetLineMetrics(Some(&mut metrics), &mut count)?;
    }
    metrics.truncate(count as usize);
    Ok(metrics)
}

fn normalized_range(anchor: u32, active: u32, text_len: u32) -> (u32, u32) {
    let start = anchor.min(active).min(text_len);
    let end = anchor.max(active).min(text_len);
    (start, end)
}

fn block_spacing(kind: &BlockKind) -> (f32, f32, f32) {
    match kind {
        BlockKind::Heading(1) => (0.0, 20.0, 10.0),
        BlockKind::Heading(2) => (0.0, 18.0, 8.0),
        BlockKind::Heading(_) => (0.0, 14.0, 6.0),
        BlockKind::Paragraph => (0.0, 4.0, 10.0),
        BlockKind::Quote => (22.0, 6.0, 12.0),
        BlockKind::ListItem { depth } => ((*depth as f32 - 1.0) * 22.0, 2.0, 4.0),
        BlockKind::Code => (12.0, 8.0, 18.0),
        BlockKind::TableRow { .. } => (12.0, 0.0, 1.0),
        BlockKind::Image { .. } => (0.0, 10.0, 14.0),
        BlockKind::Rule => (0.0, 8.0, 8.0),
    }
}

fn apply_block_style(
    layout: &IDWriteTextLayout,
    kind: &BlockKind,
    length: u32,
    body_size: f32,
) -> Result<f32> {
    let range = DWRITE_TEXT_RANGE {
        startPosition: 0,
        length,
    };
    let font_size = match kind {
        BlockKind::Heading(1) => body_size * 2.0,
        BlockKind::Heading(2) => body_size * 1.6,
        BlockKind::Heading(3) => body_size * 1.3,
        BlockKind::Heading(_) => body_size * 1.12,
        BlockKind::Code | BlockKind::TableRow { .. } => (body_size - 2.0).max(12.0),
        BlockKind::Image { .. } => (body_size - 3.0).max(12.0),
        _ => body_size,
    };
    unsafe {
        layout.SetFontSize(font_size, range)?;
        match kind {
            BlockKind::Heading(_) => {
                layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
            }
            BlockKind::Code | BlockKind::TableRow { .. } => {
                layout.SetFontFamilyName(w!("Consolas"), range)?;
                if matches!(kind, BlockKind::TableRow { header: true }) {
                    layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
                }
            }
            _ => {}
        }
    }
    Ok(font_size)
}

fn apply_line_spacing(
    layout: &IDWriteTextLayout,
    choice: LineSpacingChoice,
    font_size: f32,
) -> Result<()> {
    let multiplier = match choice {
        LineSpacingChoice::Standard => return Ok(()),
        LineSpacingChoice::Compact => 1.2,
        LineSpacingChoice::Relaxed => 1.7,
    };
    unsafe {
        layout.SetLineSpacing(
            DWRITE_LINE_SPACING_METHOD_UNIFORM,
            font_size * multiplier,
            font_size,
        )?;
    }
    Ok(())
}

fn apply_inline_styles(layout: &IDWriteTextLayout, spans: &[StyleSpan]) -> Result<()> {
    unsafe {
        for span in spans {
            let range = DWRITE_TEXT_RANGE {
                startPosition: span.start,
                length: span.length,
            };
            if span.style.bold {
                layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
            }
            if span.style.italic {
                layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range)?;
            }
            if span.style.code {
                layout.SetFontFamilyName(w!("Consolas"), range)?;
            }
            if span.style.strike {
                layout.SetStrikethrough(true, range)?;
            }
            if span.style.link {
                layout.SetUnderline(true, range)?;
            }
        }
    }
    Ok(())
}

fn apply_code_language_style(
    layout: &IDWriteTextLayout,
    kind: &BlockKind,
    spans: &[StyleSpan],
    code_font_size: f32,
) -> Result<()> {
    if !matches!(kind, BlockKind::Code) {
        return Ok(());
    }
    let Some(label) = spans
        .iter()
        .find(|span| span.start == 0 && span.style.italic)
    else {
        return Ok(());
    };
    unsafe {
        layout.SetFontSize(
            (code_font_size - 2.0).max(10.0),
            DWRITE_TEXT_RANGE {
                startPosition: label.start,
                length: label.length,
            },
        )?;
    }
    Ok(())
}

fn is_remote_source(source: &str) -> bool {
    let lower = source.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("data:")
}

const fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

fn page_geometry(viewport_width: f32) -> (f32, f32) {
    let page_left = if viewport_width < 480.0 {
        PAGE_NARROW_PADDING
    } else {
        PAGE_SIDE_PADDING
    };
    (page_left, (viewport_width - page_left * 2.0).max(100.0))
}

fn create_body_format(
    dwrite: &IDWriteFactory,
    settings: ViewSettings,
) -> Result<IDWriteTextFormat> {
    let family = wide_null(settings.font.family_name());
    unsafe {
        dwrite.CreateTextFormat(
            PCWSTR(family.as_ptr()),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            settings.font_size as f32,
            w!("ja-JP"),
        )
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        DWRITE_TEXT_METRICS, FontChoice, LineSpacingChoice, PAGE_TOP_PADDING, Renderer,
        ViewSettings, line_metrics, normalized_range, page_geometry,
    };

    #[test]
    fn document_width_tracks_the_window() {
        assert_eq!(page_geometry(1200.0), (32.0, 1136.0));
        assert_eq!(page_geometry(800.0), (32.0, 736.0));
        assert_eq!(page_geometry(400.0), (16.0, 368.0));
    }

    #[test]
    fn view_settings_have_quiet_defaults() {
        let settings = ViewSettings::default();
        assert_eq!(settings.font, FontChoice::SegoeUi);
        assert_eq!(settings.font_size, 15);
        assert_eq!(settings.line_spacing, LineSpacingChoice::Standard);
    }

    #[test]
    fn selection_range_is_ordered_and_clamped() {
        assert_eq!(normalized_range(8, 3, 10), (3, 8));
        assert_eq!(normalized_range(20, 4, 10), (4, 10));
    }

    #[test]
    fn long_tokens_wrap_inside_the_document_width() {
        let document = crate::markdown::parse(&"A".repeat(512));
        let renderer = Renderer::new().unwrap();
        let layout = renderer.layout(&document, 320.0).unwrap();
        let block = &layout.blocks[0];
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            block
                .layout
                .as_ref()
                .unwrap()
                .GetMetrics(&mut metrics)
                .unwrap();
        }
        assert!(metrics.lineCount > 1);
        assert!(metrics.widthIncludingTrailingWhitespace <= block.width + 0.5);
    }

    #[test]
    fn japanese_list_item_with_inline_code_wraps_inside_the_document_width() {
        let document = crate::markdown::parse(
            "- `D:\\Projects` 直下に、説明的な名前や任意の名前で新しいプロジェクトフォルダを作成してはならない。",
        );
        let renderer = Renderer::new().unwrap();
        let layout = renderer.layout(&document, 592.0).unwrap();
        let block = &layout.blocks[0];
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            block
                .layout
                .as_ref()
                .unwrap()
                .GetMetrics(&mut metrics)
                .unwrap();
        }
        assert!(metrics.lineCount > 1);
        assert!(metrics.widthIncludingTrailingWhitespace <= block.width + 0.5);
    }

    #[test]
    fn document_reflows_when_the_window_gets_narrower() {
        let text = "Window-width reflow keeps ordinary words inside the viewport. ".repeat(40);
        let document = crate::markdown::parse(&text);
        let renderer = Renderer::new().unwrap();
        let wide = renderer.layout(&document, 900.0).unwrap();
        let narrow = renderer.layout(&document, 360.0).unwrap();
        assert!(narrow.total_height > wide.total_height);
    }

    #[test]
    fn first_block_starts_at_the_page_padding_without_extra_heading_margin() {
        let document = crate::markdown::parse("# Title");
        let renderer = Renderer::new().unwrap();
        let layout = renderer.layout(&document, 900.0).unwrap();
        assert_eq!(layout.blocks[0].y, PAGE_TOP_PADDING);
    }

    #[test]
    fn finds_the_complete_visual_line_at_a_vertical_position() {
        let document = crate::markdown::parse(
            "A visual line that wraps should be selectable independently. \
             The second visual line must not select the first one.",
        );
        let renderer = Renderer::new().unwrap();
        let layout = renderer.layout(&document, 260.0).unwrap();
        let block = &layout.blocks[0];
        let lines = line_metrics(block.layout.as_ref().unwrap()).unwrap();
        assert!(lines.len() > 1);

        let first = renderer
            .line_range_at(&layout, block.y + lines[0].height / 2.0)
            .unwrap()
            .unwrap();
        let second = renderer
            .line_range_at(&layout, block.y + lines[0].height + lines[1].height / 2.0)
            .unwrap()
            .unwrap();

        assert_eq!(first.0, block.text_start);
        assert_eq!(first.1, block.text_start + lines[0].length);
        assert_eq!(second.0, first.1);
        assert!(second.1 > second.0);
    }
}
