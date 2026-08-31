use crate::markdown::{BlockKind, Document, StyleSpan, TableAlignment};
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
#[cfg(test)]
use windows::Win32::Graphics::DirectWrite::DWRITE_LINE_METRICS;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_ITALIC,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_HIT_TEST_METRICS, DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_EMERGENCY_BREAK, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::core::{PCWSTR, Result, w};
use windows_numerics::Vector2;

const PAGE_SIDE_PADDING: f32 = 32.0;
const PAGE_NARROW_PADDING: f32 = 16.0;
const PAGE_TOP_PADDING: f32 = 28.0;
const CODE_HORIZONTAL_PADDING: f32 = 12.0;
const CODE_VERTICAL_PADDING: f32 = 12.0;
const TABLE_CELL_PADDING_X: f32 = 10.0;
const TABLE_CELL_PADDING_Y: f32 = 7.0;
const TABLE_MIN_CONTENT_WIDTH: f32 = 40.0;
// DrawTextLayout is not clipped to this box; the long-block test guards that behavior.
const TEXT_LAYOUT_BOX_HEIGHT: f32 = 100_000.0;

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
    table_cells: Vec<LayoutCell>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text_start: u32,
    text_len: u32,
}

struct LayoutCell {
    layout: IDWriteTextLayout,
    x: f32,
    y: f32,
    width: f32,
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
        let table_column_widths = self.table_column_widths(document, page_width)?;
        let mut y = PAGE_TOP_PADDING;
        let mut blocks = Vec::with_capacity(document.blocks.len());
        let mut plain_text = Vec::new();

        for (block_index, block) in document.blocks.iter().enumerate() {
            if !blocks.is_empty() {
                plain_text.push('\n' as u16);
            }
            let text_start = plain_text.len() as u32;
            if matches!(block.kind, BlockKind::Rule) {
                blocks.push(LayoutBlock {
                    kind: block.kind.clone(),
                    layout: None,
                    table_cells: Vec::new(),
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

            if let (
                BlockKind::TableRow {
                    cells, alignments, ..
                },
                Some(column_widths),
            ) = (&block.kind, &table_column_widths[block_index])
            {
                let continues_table = block_index > 0
                    && matches!(
                        &document.blocks[block_index - 1].kind,
                        BlockKind::TableRow { .. }
                    );
                if !blocks.is_empty() && !continues_table {
                    y += 8.0;
                }

                let mut table_cells = Vec::with_capacity(column_widths.len());
                let mut column_left = page_left;
                let mut row_content_height: f32 = 18.0;
                for (column_index, content_width) in column_widths.iter().copied().enumerate() {
                    if column_index > 0 {
                        plain_text.push('\t' as u16);
                    }
                    let cell_text = cells.get(column_index).map(String::as_str).unwrap_or("");
                    let wide: Vec<u16> = cell_text.encode_utf16().collect();
                    let cell_text_start = plain_text.len() as u32;
                    plain_text.extend_from_slice(&wide);
                    let text_layout = unsafe {
                        self.dwrite.CreateTextLayout(
                            &wide,
                            &self.body_format,
                            content_width,
                            TEXT_LAYOUT_BOX_HEIGHT,
                        )?
                    };
                    unsafe {
                        text_layout.SetWordWrapping(DWRITE_WORD_WRAPPING_EMERGENCY_BREAK)?;
                        text_layout.SetTextAlignment(match alignments.get(column_index) {
                            Some(TableAlignment::Center) => DWRITE_TEXT_ALIGNMENT_CENTER,
                            Some(TableAlignment::Right) => DWRITE_TEXT_ALIGNMENT_TRAILING,
                            _ => DWRITE_TEXT_ALIGNMENT_LEADING,
                        })?;
                    }
                    let font_size = apply_block_style(
                        &text_layout,
                        &block.kind,
                        wide.len() as u32,
                        self.settings.font_size as f32,
                    )?;
                    apply_line_spacing(&text_layout, self.settings.line_spacing, font_size)?;

                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    unsafe { text_layout.GetMetrics(&mut metrics)? };
                    let cell_height = metrics.height.max(18.0);
                    row_content_height = row_content_height.max(cell_height);
                    table_cells.push(LayoutCell {
                        layout: text_layout,
                        x: column_left + TABLE_CELL_PADDING_X,
                        y: y + TABLE_CELL_PADDING_Y,
                        width: content_width,
                        text_start: cell_text_start,
                        text_len: wide.len() as u32,
                    });
                    column_left += content_width + TABLE_CELL_PADDING_X * 2.0;
                }

                let row_height = row_content_height + TABLE_CELL_PADDING_Y * 2.0;
                let text_len = plain_text.len() as u32 - text_start;
                blocks.push(LayoutBlock {
                    kind: block.kind.clone(),
                    layout: None,
                    table_cells,
                    x: page_left,
                    y,
                    width: page_width,
                    height: row_height,
                    text_start,
                    text_len,
                });
                let ends_table = block_index + 1 == document.blocks.len()
                    || !matches!(
                        &document.blocks[block_index + 1].kind,
                        BlockKind::TableRow { .. }
                    );
                y += row_height + if ends_table { 14.0 } else { 0.0 };
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
                        format!("[remote image omitted] {label}\n{source}")
                    } else {
                        format!("[image] {label}\n{source}")
                    }
                }
                _ => block.text.clone(),
            };

            let wide: Vec<u16> = visible_text.encode_utf16().collect();
            plain_text.extend_from_slice(&wide);
            let text_layout = unsafe {
                self.dwrite.CreateTextLayout(
                    &wide,
                    &self.body_format,
                    content_width,
                    TEXT_LAYOUT_BOX_HEIGHT,
                )?
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
                table_cells: Vec::new(),
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

    fn table_column_widths(
        &self,
        document: &Document,
        page_width: f32,
    ) -> Result<Vec<Option<Vec<f32>>>> {
        let mut widths_by_block = vec![None; document.blocks.len()];
        let mut start = 0usize;
        while start < document.blocks.len() {
            if !matches!(document.blocks[start].kind, BlockKind::TableRow { .. }) {
                start += 1;
                continue;
            }
            let mut end = start;
            let mut column_count = 0usize;
            while end < document.blocks.len() {
                let BlockKind::TableRow { cells, .. } = &document.blocks[end].kind else {
                    break;
                };
                column_count = column_count.max(cells.len());
                end += 1;
            }
            if column_count == 0 {
                start = end;
                continue;
            }

            let mut natural_widths = vec![0.0f32; column_count];
            for block in &document.blocks[start..end] {
                let BlockKind::TableRow { cells, .. } = &block.kind else {
                    continue;
                };
                for (column_index, cell) in cells.iter().enumerate() {
                    natural_widths[column_index] = natural_widths[column_index]
                        .max(self.measure_table_cell(cell, &block.kind)?);
                }
            }
            let available_content_width = (page_width
                - TABLE_CELL_PADDING_X * 2.0 * column_count as f32)
                .max(column_count as f32);
            let fitted = fit_column_widths(&natural_widths, available_content_width);
            for slot in &mut widths_by_block[start..end] {
                *slot = Some(fitted.clone());
            }
            start = end;
        }
        Ok(widths_by_block)
    }

    fn measure_table_cell(&self, text: &str, kind: &BlockKind) -> Result<f32> {
        if text.is_empty() {
            return Ok(0.0);
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let text_layout = unsafe {
            self.dwrite.CreateTextLayout(
                &wide,
                &self.body_format,
                TEXT_LAYOUT_BOX_HEIGHT,
                TEXT_LAYOUT_BOX_HEIGHT,
            )?
        };
        apply_block_style(
            &text_layout,
            kind,
            wide.len() as u32,
            self.settings.font_size as f32,
        )?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { text_layout.GetMetrics(&mut metrics)? };
        Ok(metrics.widthIncludingTrailingWhitespace.ceil())
    }

    pub fn hit_test(&self, document: &LayoutDocument, x: f32, y: f32) -> Result<u32> {
        let mut text_blocks = document.blocks.iter().filter(|block| {
            (block.layout.is_some() || !block.table_cells.is_empty()) && block.text_len > 0
        });
        let Some(first) = text_blocks.next() else {
            return Ok(0);
        };
        if y <= first.y {
            return Ok(first.text_start);
        }

        for block in std::iter::once(first).chain(text_blocks) {
            if y < block.y {
                return Ok(block.text_start);
            }
            if y <= block.y + block.height {
                if !block.table_cells.is_empty() {
                    let cell = block
                        .table_cells
                        .iter()
                        .find(|cell| x < cell.x + cell.width + TABLE_CELL_PADDING_X)
                        .or_else(|| block.table_cells.last())
                        .expect("table rows always contain a cell");
                    let mut trailing = windows::core::BOOL::default();
                    let mut inside = windows::core::BOOL::default();
                    let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                    unsafe {
                        cell.layout.HitTestPoint(
                            x - cell.x,
                            y - cell.y,
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
                    let local = (metrics.textPosition + trailing_length).min(cell.text_len);
                    return Ok(cell.text_start + local);
                }
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

    pub fn sentence_range_at(
        &self,
        document: &LayoutDocument,
        x: f32,
        y: f32,
    ) -> Result<Option<(u32, u32)>> {
        for block in &document.blocks {
            if y < block.y || y > block.y + block.height {
                continue;
            }

            let (text_start, text_len, character_position) = if !block.table_cells.is_empty() {
                let Some(cell) = block
                    .table_cells
                    .iter()
                    .find(|cell| x < cell.x + cell.width + TABLE_CELL_PADDING_X)
                    .or_else(|| block.table_cells.last())
                else {
                    return Ok(None);
                };
                if cell.text_len == 0 {
                    return Ok(None);
                }
                (
                    cell.text_start,
                    cell.text_len,
                    hit_test_character(&cell.layout, x - cell.x, y - cell.y, cell.text_len)?,
                )
            } else {
                let Some(text_layout) = block.layout.as_ref() else {
                    return Ok(None);
                };
                if block.text_len == 0 {
                    return Ok(None);
                }
                (
                    block.text_start,
                    block.text_len,
                    hit_test_character(text_layout, x - block.x, y - block.y, block.text_len)?,
                )
            };

            let text_end = text_start + text_len;
            let text = &document.plain_text[text_start as usize..text_end as usize];
            return Ok(sentence_range_utf16(text, character_position)
                .map(|(start, end)| (text_start + start, text_start + end)));
        }
        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
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

                match &block.kind {
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
                    BlockKind::TableRow { .. } => {
                        let bounds = D2D_RECT_F {
                            left: block.x,
                            top: draw_y,
                            right: block.x + block.width,
                            bottom: draw_y + block.height,
                        };
                        target.FillRectangle(&bounds, &resources.panel_brush);
                        target.DrawRectangle(&bounds, &resources.line_brush, 1.0, None);
                        for cell in block.table_cells.iter().skip(1) {
                            let boundary_x = cell.x - TABLE_CELL_PADDING_X;
                            target.DrawLine(
                                Vector2 {
                                    X: boundary_x,
                                    Y: draw_y,
                                },
                                Vector2 {
                                    X: boundary_x,
                                    Y: draw_y + block.height,
                                },
                                &resources.line_brush,
                                1.0,
                                None,
                            );
                        }
                    }
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
                            text_layout,
                            block.x,
                            draw_y,
                            block.text_start,
                            block.text_len,
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
                for cell in &block.table_cells {
                    let cell_draw_y = cell.y - scroll_y;
                    if let Some(selection) = selection {
                        draw_selection(
                            target,
                            &resources.selection_brush,
                            &cell.layout,
                            cell.x,
                            cell_draw_y,
                            cell.text_start,
                            cell.text_len,
                            selection,
                        )?;
                    }
                    target.DrawTextLayout(
                        Vector2 {
                            X: cell.x,
                            Y: cell_draw_y,
                        },
                        &cell.layout,
                        &resources.text_brush,
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

    pub fn find_text(
        &self,
        query: &[u16],
        start: u32,
        forward: bool,
        match_case: bool,
        whole_word: bool,
    ) -> Option<(u32, u32)> {
        find_utf16(
            &self.plain_text,
            query,
            start.min(self.text_len()),
            forward,
            match_case,
            whole_word,
        )
    }

    pub fn vertical_range_for_position(&self, position: u32) -> Result<Option<(f32, f32)>> {
        let position = position.min(self.text_len());
        for block in &self.blocks {
            let block_end = block.text_start + block.text_len;
            if position < block.text_start || position > block_end {
                continue;
            }

            if !block.table_cells.is_empty() {
                let Some(cell) = block
                    .table_cells
                    .iter()
                    .find(|cell| {
                        position >= cell.text_start && position <= cell.text_start + cell.text_len
                    })
                    .or_else(|| block.table_cells.last())
                else {
                    return Ok(None);
                };
                let local = position.saturating_sub(cell.text_start).min(cell.text_len);
                let mut point_x = 0.0;
                let mut point_y = 0.0;
                let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                unsafe {
                    cell.layout.HitTestTextPosition(
                        local,
                        false,
                        &mut point_x,
                        &mut point_y,
                        &mut metrics,
                    )?;
                }
                let top = cell.y + point_y;
                return Ok(Some((top, top + metrics.height)));
            }

            let Some(text_layout) = block.layout.as_ref() else {
                return Ok(None);
            };
            let local = position
                .saturating_sub(block.text_start)
                .min(block.text_len);
            let mut point_x = 0.0;
            let mut point_y = 0.0;
            let mut metrics = DWRITE_HIT_TEST_METRICS::default();
            unsafe {
                text_layout.HitTestTextPosition(
                    local,
                    false,
                    &mut point_x,
                    &mut point_y,
                    &mut metrics,
                )?;
            }
            let top = block.y + point_y;
            return Ok(Some((top, top + metrics.height)));
        }
        Ok(None)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_selection(
    target: &ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    text_layout: &IDWriteTextLayout,
    x: f32,
    draw_y: f32,
    text_start: u32,
    text_len: u32,
    selection: (u32, u32),
) -> Result<()> {
    let (selection_start, selection_end) = normalized_range(selection.0, selection.1, u32::MAX);
    let text_end = text_start + text_len;
    let start = selection_start.max(text_start);
    let end = selection_end.min(text_end);
    if start >= end {
        return Ok(());
    }

    let local_start = start - text_start;
    let local_length = end - start;
    let initial_capacity = (local_length as usize).clamp(1, 16);
    let mut metrics = vec![DWRITE_HIT_TEST_METRICS::default(); initial_capacity];
    let mut count = 0u32;
    let first_result = unsafe {
        text_layout.HitTestTextRange(
            local_start,
            local_length,
            x,
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
                x,
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

#[cfg(test)]
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

fn hit_test_character(layout: &IDWriteTextLayout, x: f32, y: f32, text_len: u32) -> Result<u32> {
    let mut trailing = windows::core::BOOL::default();
    let mut inside = windows::core::BOOL::default();
    let mut metrics = DWRITE_HIT_TEST_METRICS::default();
    unsafe {
        layout.HitTestPoint(x, y, &mut trailing, &mut inside, &mut metrics)?;
    }
    Ok(metrics.textPosition.min(text_len.saturating_sub(1)))
}

fn sentence_range_utf16(text: &[u16], position: u32) -> Option<(u32, u32)> {
    if text.is_empty() {
        return None;
    }

    let target = position.min(text.len().saturating_sub(1) as u32) as usize;
    let mut cursor = 0usize;
    let mut last_range = None;

    while cursor < text.len() {
        while cursor < text.len() && is_sentence_whitespace(text[cursor]) {
            cursor += 1;
        }
        if cursor == text.len() {
            break;
        }

        let start = cursor;
        while cursor < text.len() && text[cursor] != '\n' as u16 {
            if is_sentence_terminal(text, cursor) {
                cursor += 1;
                while cursor < text.len() && is_sentence_terminal(text, cursor) {
                    cursor += 1;
                }
                while cursor < text.len() && is_sentence_closer(text[cursor]) {
                    cursor += 1;
                }
                break;
            }
            cursor += 1;
        }

        let mut end = cursor;
        while end > start && is_sentence_whitespace(text[end - 1]) {
            end -= 1;
        }
        if start < end {
            let range = (start as u32, end as u32);
            if target < end {
                return Some(range);
            }
            last_range = Some(range);
        }

        if cursor < text.len() && text[cursor] == '\n' as u16 {
            cursor += 1;
        }
    }

    last_range
}

fn is_sentence_terminal(text: &[u16], index: usize) -> bool {
    match char::from_u32(text[index] as u32) {
        Some('。' | '！' | '？' | '!' | '?') => true,
        Some('.') => {
            let previous = index.checked_sub(1).and_then(|index| text.get(index));
            let next = text.get(index + 1);
            !matches!(
                (previous, next),
                (Some(previous), Some(next))
                    if is_ascii_alphanumeric_utf16(*previous)
                        && is_ascii_alphanumeric_utf16(*next)
            )
        }
        _ => false,
    }
}

fn is_ascii_alphanumeric_utf16(value: u16) -> bool {
    char::from_u32(value as u32).is_some_and(|value| value.is_ascii_alphanumeric())
}

fn is_sentence_closer(value: u16) -> bool {
    matches!(
        char::from_u32(value as u32),
        Some(
            '」' | '』'
                | '）'
                | '】'
                | '〕'
                | '〉'
                | '》'
                | '”'
                | '’'
                | '"'
                | '\''
                | ')'
                | ']'
                | '}'
        )
    )
}

fn is_sentence_whitespace(value: u16) -> bool {
    char::from_u32(value as u32).is_some_and(char::is_whitespace)
}

#[derive(Clone, Copy)]
struct FoldedChar {
    value: char,
    start: u32,
    end: u32,
}

fn fold_utf16(value: &[u16], match_case: bool) -> Vec<FoldedChar> {
    let mut result = Vec::with_capacity(value.len());
    let mut offset = 0u32;
    for decoded in std::char::decode_utf16(value.iter().copied()) {
        let character = decoded.unwrap_or(char::REPLACEMENT_CHARACTER);
        let length = character.len_utf16() as u32;
        if match_case {
            result.push(FoldedChar {
                value: character,
                start: offset,
                end: offset + length,
            });
        } else {
            for folded in character.to_lowercase() {
                result.push(FoldedChar {
                    value: folded,
                    start: offset,
                    end: offset + length,
                });
            }
        }
        offset += length;
    }
    result
}

fn find_utf16(
    text: &[u16],
    query: &[u16],
    start: u32,
    forward: bool,
    match_case: bool,
    whole_word: bool,
) -> Option<(u32, u32)> {
    if query.is_empty() {
        return None;
    }
    let text = fold_utf16(text, match_case);
    let query = fold_utf16(query, match_case)
        .into_iter()
        .map(|item| item.value)
        .collect::<Vec<_>>();
    if query.is_empty() || query.len() > text.len() {
        return None;
    }

    let is_word = |value: char| value.is_alphanumeric() || value == '_';
    let matches_at = |index: usize| {
        text[index..index + query.len()]
            .iter()
            .zip(&query)
            .all(|(left, right)| left.value == *right)
            && (!whole_word
                || (index == 0 || !is_word(text[index - 1].value))
                    && (index + query.len() == text.len()
                        || !is_word(text[index + query.len()].value)))
    };
    let range_at = |index: usize| (text[index].start, text[index + query.len() - 1].end);

    if forward {
        let mut wrapped = None;
        for index in 0..=text.len() - query.len() {
            if !matches_at(index) {
                continue;
            }
            let range = range_at(index);
            if range.0 >= start {
                return Some(range);
            }
            wrapped.get_or_insert(range);
        }
        wrapped
    } else {
        let mut wrapped = None;
        for index in (0..=text.len() - query.len()).rev() {
            if !matches_at(index) {
                continue;
            }
            let range = range_at(index);
            if range.1 <= start {
                return Some(range);
            }
            wrapped.get_or_insert(range);
        }
        wrapped
    }
}

fn fit_column_widths(natural: &[f32], available: f32) -> Vec<f32> {
    if natural.is_empty() {
        return Vec::new();
    }

    let count = natural.len() as f32;
    let available = available.max(count);
    let minimum = TABLE_MIN_CONTENT_WIDTH.min(available / count).max(1.0);
    let preferred = natural
        .iter()
        .map(|width| width.max(minimum))
        .collect::<Vec<_>>();
    let preferred_total = preferred.iter().sum::<f32>();
    let mut widths = if preferred_total <= available {
        let extra = (available - preferred_total) / count;
        preferred.iter().map(|width| width + extra).collect()
    } else {
        let distributable = (available - minimum * count).max(0.0);
        let weights = preferred
            .iter()
            .map(|width| (width - minimum).max(0.0))
            .collect::<Vec<_>>();
        let weight_total = weights.iter().sum::<f32>();
        if weight_total <= f32::EPSILON {
            vec![available / count; natural.len()]
        } else {
            weights
                .iter()
                .map(|weight| minimum + distributable * weight / weight_total)
                .collect()
        }
    };

    let used = widths.iter().take(widths.len() - 1).sum::<f32>();
    if let Some(last) = widths.last_mut() {
        *last = (available - used).max(1.0);
    }
    widths
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
        BlockKind::Code => (body_size - 2.0).max(12.0),
        BlockKind::TableRow { .. } => (body_size - 1.0).max(11.0),
        BlockKind::Image { .. } => (body_size - 3.0).max(12.0),
        _ => body_size,
    };
    unsafe {
        layout.SetFontSize(font_size, range)?;
        match kind {
            BlockKind::Heading(_) => {
                layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
            }
            BlockKind::Code => {
                layout.SetFontFamilyName(w!("Consolas"), range)?;
            }
            BlockKind::TableRow { header: true, .. } => {
                layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
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
        TEXT_LAYOUT_BOX_HEIGHT, ViewSettings, find_utf16, fit_column_widths, line_metrics,
        normalized_range, page_geometry, sentence_range_utf16,
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
    fn search_wraps_in_both_directions_and_respects_options() {
        let text = "Alpha beta ALPHA alphabet"
            .encode_utf16()
            .collect::<Vec<_>>();
        let alpha = "alpha".encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            find_utf16(&text, &alpha, 0, true, false, false),
            Some((0, 5))
        );
        assert_eq!(
            find_utf16(&text, &alpha, 6, true, false, false),
            Some((11, 16))
        );
        assert_eq!(
            find_utf16(&text, &alpha, 11, false, false, false),
            Some((0, 5))
        );
        assert_eq!(
            find_utf16(&text, &alpha, 0, false, false, false),
            Some((17, 22))
        );
        assert_eq!(
            find_utf16(&text, &alpha, 0, true, true, false),
            Some((17, 22))
        );
        assert_eq!(find_utf16(&text, &alpha, 0, true, true, true), None);
        assert_eq!(
            find_utf16(&text, &alpha, 17, true, false, true),
            Some((0, 5))
        );
    }

    #[test]
    fn table_columns_fit_the_available_width_and_preserve_emphasis() {
        let widths = fit_column_widths(&[50.0, 300.0, 80.0], 360.0);
        assert!((widths.iter().sum::<f32>() - 360.0).abs() < 0.01);
        assert!(widths[1] > widths[0]);
        assert!(widths[1] > widths[2]);

        let narrow = fit_column_widths(&[50.0, 300.0, 80.0], 60.0);
        assert!((narrow.iter().sum::<f32>() - 60.0).abs() < 0.01);
        assert!(narrow.iter().all(|width| *width > 0.0));
    }

    #[test]
    fn markdown_table_uses_shared_columns_and_tab_separated_copy_text() {
        let document = crate::markdown::parse(
            "| 軸 | タグ | 対象 | 灯数 |\n| --- | --- | --- | ---: |\n| 灯種 | BEAM | 全ムービングビーム | 164 |\n| 大分類 | ARCH | 門型アーチ全灯 | 80 |",
        );
        let renderer = Renderer::new().unwrap();
        let layout = renderer.layout(&document, 900.0).unwrap();

        assert_eq!(layout.blocks.len(), 3);
        assert!(layout.blocks.iter().all(|block| block.layout.is_none()));
        assert!(
            layout
                .blocks
                .iter()
                .all(|block| block.table_cells.len() == 4)
        );
        for column in 0..4 {
            let x = layout.blocks[0].table_cells[column].x;
            assert!(
                layout
                    .blocks
                    .iter()
                    .all(|block| (block.table_cells[column].x - x).abs() < 0.01)
            );
        }

        let first = &layout.blocks[0];
        let copied = String::from_utf16(
            &layout.selected_text(first.text_start, first.text_start + first.text_len),
        )
        .unwrap();
        assert_eq!(copied, "軸\tタグ\t対象\t灯数");
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
    fn a_single_code_block_can_exceed_the_layout_box_height() {
        let markdown = format!("```text\n{}```", "line\n".repeat(8_000));
        let document = crate::markdown::parse(&markdown);
        let renderer = Renderer::new().unwrap();
        let layout = renderer.layout(&document, 900.0).unwrap();
        let block = &layout.blocks[0];
        let lines = line_metrics(block.layout.as_ref().unwrap()).unwrap();
        assert!(lines.len() >= 8_000);
        assert!(block.height > TEXT_LAYOUT_BOX_HEIGHT);
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
    fn finds_the_sentence_containing_the_clicked_character() {
        fn selected_sentence(text: &str, marker: &str) -> String {
            let marker_byte = text.find(marker).unwrap();
            let position = text[..marker_byte].encode_utf16().count() as u32;
            let wide = text.encode_utf16().collect::<Vec<_>>();
            let (start, end) = sentence_range_utf16(&wide, position).unwrap();
            String::from_utf16(&wide[start as usize..end as usize]).unwrap()
        }

        let text = "「最初です。」 次の文は表示上で折り返しても一続きです！ 最後です？";
        assert_eq!(selected_sentence(text, "最初"), "「最初です。」");
        assert_eq!(
            selected_sentence(text, "折り返して"),
            "次の文は表示上で折り返しても一続きです！"
        );
        assert_eq!(selected_sentence(text, "最後"), "最後です？");
    }

    #[test]
    fn keeps_decimal_points_inside_a_sentence_and_stops_at_hard_breaks() {
        let text = "Version 1.2 is current.\nSecond sentence.";
        let wide = text.encode_utf16().collect::<Vec<_>>();
        let decimal_position = text.find("1.2").unwrap() as u32;
        let second_position = text.find("Second").unwrap() as u32;

        let first = sentence_range_utf16(&wide, decimal_position).unwrap();
        let second = sentence_range_utf16(&wide, second_position).unwrap();
        assert_eq!(
            String::from_utf16(&wide[first.0 as usize..first.1 as usize]).unwrap(),
            "Version 1.2 is current."
        );
        assert_eq!(
            String::from_utf16(&wide[second.0 as usize..second.1 as usize]).unwrap(),
            "Second sentence."
        );
    }
}
