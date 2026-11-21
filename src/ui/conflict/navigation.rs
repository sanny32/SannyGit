use super::*;

pub(super) fn change_view(layout: Layout) -> usize {
    match layout {
        Layout::OursResult => 0,
        Layout::ResultTheirs => 1,
        Layout::ThreeWay | Layout::ResultBelow => 2,
    }
}

pub(super) fn change_navigation(ui: &mut Ui, dialog: &mut Dialog, style: ToolbarStyle, busy: bool) {
    if let Some(comparison) = &dialog.draft_comparison {
        let ranges = comparison.change_ranges[change_view(dialog.layout)].clone();
        draft_navigation(ui, dialog, style, busy, &ranges, false);
        return;
    }
    let (previous, next) = if let Some(comparison) = &dialog.comparison {
        let anchor = selected_range(dialog, comparison)
            .map(|range| range.start)
            .or(dialog.change_row);
        let ranges = &comparison.change_ranges[change_view(dialog.layout)];
        let previous = anchor.and_then(|anchor| {
            ranges
                .partition_point(|range| range.start < anchor)
                .checked_sub(1)
                .map(|index| ranges[index].start)
        });
        let next = ranges
            .get(anchor.map_or(0, |anchor| {
                ranges.partition_point(|range| range.start <= anchor)
            }))
            .map(|range| range.start);
        (previous, next)
    } else {
        (None, None)
    };
    let labels = if style.compact {
        ["Prev.", "Next"]
    } else {
        ["Prev. Change", "Next Change"]
    };
    if let Some(row) = navigation_buttons(
        ui,
        style,
        [previous, next],
        labels,
        ["Previous change", "Next change"],
        !busy && dialog.draft.is_none(),
        false,
    ) {
        dialog.change_row = Some(row);
        dialog.conflict_navigation_row = Some(row);
        dialog.jump_to_row = Some(row);
        if let Some(conflict) = dialog
            .comparison
            .as_ref()
            .and_then(|comparison| comparison.rows[row].conflict)
        {
            dialog.selected = Some(conflict);
        }
    }
}

pub(super) fn conflict_navigation(
    ui: &mut Ui,
    dialog: &mut Dialog,
    style: ToolbarStyle,
    busy: bool,
) {
    if let Some(comparison) = &dialog.draft_comparison {
        let mut ranges = comparison.conflict_rows.clone();
        if ranges.is_empty() {
            ranges.extend(dialog.draft_recovered_spans.iter().flatten().map(|span| {
                let mut rows = comparison.rows_for_result_span(span);
                if rows.is_empty() && !comparison.rows.is_empty() {
                    rows.start = rows.start.min(comparison.rows.len() - 1);
                    rows.end = rows.start + 1;
                }
                rows
            }));
        }
        draft_navigation(ui, dialog, style, busy, &ranges, true);
        return;
    }
    let enabled = !busy && dialog.draft.is_none() && !dialog.show_base;
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    let viewport_row = (dialog.scroll_y[0] / row_height) as usize;
    let targets = dialog.comparison.as_ref().map_or([None; 2], |comparison| {
        let row = dialog.conflict_navigation_row.unwrap_or(viewport_row);
        let has_unresolved = dialog.edits.iter().any(Option::is_none);
        let eligible =
            |index| !has_unresolved || dialog.edits.get(index).is_some_and(Option::is_none);
        [
            comparison
                .conflict_rows
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, range)| (range.end <= row && eligible(index)).then_some(index)),
            comparison
                .conflict_rows
                .iter()
                .enumerate()
                .find_map(|(index, range)| {
                    let after = if dialog.conflict_navigation_row.is_some() {
                        range.start > row
                    } else {
                        range.end > row
                    };
                    (after && eligible(index)).then_some(index)
                }),
        ]
    });
    let labels = if style.compact {
        ["Prev.", "Next"]
    } else {
        ["Prev. Conflict", "Next Conflict"]
    };
    if let Some(index) = navigation_buttons(
        ui,
        style,
        targets,
        labels,
        ["Previous conflict block", "Next conflict block"],
        enabled,
        true,
    ) {
        dialog.go_to_conflict(index);
    }
}

pub(super) fn draft_caret_row(dialog: &Dialog, comparison: &Comparison) -> Option<usize> {
    let (_, line) = dialog.manual_cursor?;
    Some(
        comparison
            .result_row_boundaries
            .partition_point(|&boundary| boundary < line)
            .saturating_sub(1),
    )
}

fn draft_navigation(
    ui: &mut Ui,
    dialog: &mut Dialog,
    style: ToolbarStyle,
    busy: bool,
    ranges: &[std::ops::Range<usize>],
    conflict: bool,
) {
    let Some(comparison) = &dialog.draft_comparison else {
        return;
    };
    let anchor = dialog
        .draft_selection
        .as_ref()
        .map(|range| range.start)
        .or_else(|| draft_caret_row(dialog, comparison));
    let targets = [
        anchor.and_then(|row| {
            ranges
                .iter()
                .rposition(|range| !range.is_empty() && range.end <= row)
        }),
        ranges
            .iter()
            .position(|range| !range.is_empty() && anchor.is_none_or(|row| range.start > row)),
    ];
    let labels = if style.compact {
        ["Prev.", "Next"]
    } else if conflict {
        ["Prev. Conflict", "Next Conflict"]
    } else {
        ["Prev. Change", "Next Change"]
    };
    let tips = if conflict {
        ["Previous conflict block", "Next conflict block"]
    } else {
        ["Previous change", "Next change"]
    };
    if let Some(index) = navigation_buttons(ui, style, targets, labels, tips, !busy, conflict) {
        let range = ranges[index].clone();
        let offset = comparison.result_offset_at(range.start);
        dialog.manual_edit_start = Some(comparison.result[..offset].chars().count());
        dialog.jump_to_row = Some(range.start);
        dialog.draft_selection = Some(range);
    }
}

fn navigation_buttons(
    ui: &mut Ui,
    style: ToolbarStyle,
    targets: [Option<usize>; 2],
    labels: [&str; 2],
    tooltips: [&str; 2],
    enabled: bool,
    conflict: bool,
) -> Option<usize> {
    let order = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        [1, 0]
    } else {
        [0, 1]
    };
    let mut selected = None;
    for index in order {
        let icon = ToolIcon::Navigate {
            previous: index == 0,
            conflict,
        };
        if tool(
            ui,
            style,
            icon,
            labels[index],
            enabled && targets[index].is_some(),
            false,
        )
        .on_hover_text(tooltips[index])
        .clicked()
        {
            selected = targets[index];
        }
    }
    selected
}

pub(super) fn selected_range<'a>(
    dialog: &'a Dialog,
    comparison: &'a Comparison,
) -> Option<&'a std::ops::Range<usize>> {
    if dialog.draft.is_some() {
        dialog.draft_selection.as_ref()
    } else if let Some(row) = dialog.change_row {
        let ranges = &comparison.change_ranges[change_view(dialog.layout)];
        let index = ranges.partition_point(|range| range.end <= row);
        ranges.get(index).filter(|range| range.contains(&row))
    } else {
        dialog
            .selected
            .and_then(|index| comparison.conflict_rows.get(index))
    }
}
