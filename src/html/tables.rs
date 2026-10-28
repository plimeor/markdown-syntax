//! GFM table rendering (attribute `align="…"` form).

use alloc::string::String;

use crate::ast::{Table, TableAlignment};

use super::inlines::render_inlines;
use super::Ctx;

/// Column alignment attribute (GFM attribute form, NOT `style=`).
pub fn table_align_attr(a: TableAlignment) -> &'static str {
    match a {
        TableAlignment::None => "",
        TableAlignment::Left => " align=\"left\"",
        TableAlignment::Right => " align=\"right\"",
        TableAlignment::Center => " align=\"center\"",
    }
}

fn alignment_for(table: &Table, col: usize) -> TableAlignment {
    table
        .alignments
        .get(col)
        .copied()
        .unwrap_or(TableAlignment::None)
}

/// Render a full GFM table into `out`. The header row drives the column count;
/// body cells beyond that width are dropped and missing cells render as empty
/// tags.
pub fn render_table(table: &Table, ctx: &Ctx, out: &mut String) {
    out.push_str("<table>\n<thead>\n<tr>\n");

    let header_cells = table.rows.first().map_or(&[][..], |r| &r.cells[..]);
    let width = header_cells.len();

    for (col, cell) in header_cells.iter().enumerate() {
        if col > 0 {
            out.push('\n');
        }
        out.push_str("<th");
        out.push_str(table_align_attr(alignment_for(table, col)));
        out.push('>');
        render_inlines(&cell.children, ctx, out);
        out.push_str("</th>");
    }
    out.push_str("\n</tr>\n</thead>");

    if table.rows.len() > 1 {
        out.push_str("\n<tbody>\n");
        for (i, row) in table.rows[1..].iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str("<tr>\n");
            for col in 0..width {
                if col > 0 {
                    out.push('\n');
                }
                out.push_str("<td");
                out.push_str(table_align_attr(alignment_for(table, col)));
                out.push('>');
                if let Some(cell) = row.cells.get(col) {
                    render_inlines(&cell.children, ctx, out);
                }
                out.push_str("</td>");
            }
            out.push_str("\n</tr>");
        }
        out.push_str("\n</tbody>\n");
    } else {
        out.push('\n');
    }

    out.push_str("</table>");
}
