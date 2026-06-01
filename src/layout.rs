use crate::config::{Split, WidgetKind};
use anyhow::Result;
use ratatui::layout::{Constraint, Layout, Rect};

/// Resolve the configured split tree into concrete widget placements for `area`.
pub fn resolve(split: &Split, area: Rect) -> Result<Vec<(WidgetKind, Rect)>> {
    let mut out = Vec::new();
    resolve_split(split, area, &mut out)?;
    Ok(out)
}

fn resolve_split(split: &Split, area: Rect, out: &mut Vec<(WidgetKind, Rect)>) -> Result<()> {
    let constraints: Vec<Constraint> = split
        .children
        .iter()
        .map(|c| c.size.to_constraint())
        .collect::<Result<_>>()?;

    let rects = Layout::default()
        .direction(split.direction.into())
        .constraints(constraints)
        .split(area);

    for (child, rect) in split.children.iter().zip(rects.iter()) {
        if let Some(widget) = child.widget {
            out.push((widget, *rect));
        } else if let Some(nested) = &child.split {
            resolve_split(nested, *rect, out)?;
        }
    }
    Ok(())
}
