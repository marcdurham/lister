use crate::model::{Item, Membership};
use uuid::Uuid;

/// Items and memberships produced by parsing a markdown file, ready to merge into the
/// current list of the app (memberships with `parent_id: None` are the new top-level
/// roots).
pub struct MarkdownImport {
    pub items: Vec<Item>,
    pub memberships: Vec<Membership>,
}

/// How a line's text classifies the item it produces.
enum LineKind {
    /// Plain text, no `TODO:`/`DONE:` prefix and not a bare `TODO` marker.
    Plain(String),
    /// `TODO: <text>` / `DONE: <text>` - always a task, regardless of context.
    Explicit { text: String, done: bool },
    /// A line that is only `TODO` or `TODO:` with nothing else: not a task itself, but
    /// every descendant under it defaults to being one.
    BareTodoMarker,
}

/// Strip `prefix` from the start of `s`, case-insensitively.
fn strip_ci_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn classify_line(text: &str) -> LineKind {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("TODO") || trimmed.eq_ignore_ascii_case("TODO:") {
        return LineKind::BareTodoMarker;
    }
    if let Some(rest) = strip_ci_prefix(trimmed, "TODO:") {
        return LineKind::Explicit {
            text: rest.trim().to_string(),
            done: false,
        };
    }
    if let Some(rest) = strip_ci_prefix(trimmed, "DONE:") {
        return LineKind::Explicit {
            text: rest.trim().to_string(),
            done: true,
        };
    }
    LineKind::Plain(trimmed.to_string())
}

/// Parse markdown text into a hierarchy of items:
/// - A heading (`#` through `######`) becomes a list item; a sub-heading nests under the
///   nearest heading of a shallower level, so the heading structure becomes the list tree.
/// - A bullet or numbered list item (`- `, `* `, `+ `, `1. `) becomes a child item, nested
///   by its indentation under either the enclosing list item or the enclosing heading.
/// - Any other non-blank line is appended to the notes of the nearest enclosing heading.
/// - A line starting with `TODO:` becomes an unchecked task; `DONE:` becomes a checked
///   one. A heading or list item that is *only* `TODO`/`TODO:` isn't a task itself, but
///   every item nested under it defaults to being one.
pub fn parse_markdown(text: &str) -> MarkdownImport {
    let mut items = Vec::new();
    let mut memberships = Vec::new();
    // Global increasing counter: since lines are processed top-to-bottom, using one
    // counter (rather than one per parent) still yields correctly increasing positions
    // among any given item's siblings.
    let mut position = 0.0_f64;

    // Chain of currently open headings, shallowest first: (level, item_id, force_task).
    let mut heading_stack: Vec<(usize, Uuid, bool)> = Vec::new();
    // Chain of currently open list items, shallowest first: (indent, item_id, force_task).
    let mut list_stack: Vec<(usize, Uuid, bool)> = Vec::new();
    // Accumulated note lines per heading item, flushed into `notes` at the end.
    let mut notes: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();

    let mut in_code_block = false;

    let add_item = |items: &mut Vec<Item>,
                         memberships: &mut Vec<Membership>,
                         position: &mut f64,
                         line_text: &str,
                         is_heading: bool,
                         inherited_force_task: bool,
                         parent: Option<Uuid>|
     -> (Uuid, bool) {
        let (text, is_list, done, force_task_for_children) = match classify_line(line_text) {
            LineKind::BareTodoMarker => ("TODO".to_string(), is_heading, false, true),
            LineKind::Explicit { text, done } => (text, false, done, inherited_force_task),
            LineKind::Plain(text) => {
                if inherited_force_task {
                    (text, false, false, true)
                } else {
                    (text, is_heading, false, false)
                }
            }
        };
        let mut item = Item::new(text, false);
        item.is_list = is_list;
        item.done = done;
        let id = item.id;
        *position += 1.0;
        let membership = Membership::new(id, parent, *position);
        items.push(item);
        memberships.push(membership);
        (id, force_task_for_children)
    };

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            if let Some(&(_, heading_id, _)) = heading_stack.last() {
                notes.entry(heading_id).or_default().push(line.to_string());
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        if let Some((level, heading_text)) = heading_line(trimmed) {
            while heading_stack.last().is_some_and(|(l, _, _)| *l >= level) {
                heading_stack.pop();
            }
            let parent = heading_stack.last().map(|(_, id, _)| *id);
            let inherited = heading_stack.last().is_some_and(|(_, _, f)| *f);
            let (id, force_task) = add_item(
                &mut items,
                &mut memberships,
                &mut position,
                &heading_text,
                true,
                inherited,
                parent,
            );
            heading_stack.push((level, id, force_task));
            list_stack.clear();
            continue;
        }

        if let Some((indent, item_text)) = list_item_line(line) {
            while list_stack.last().is_some_and(|(i, _, _)| *i >= indent) {
                list_stack.pop();
            }
            let parent = list_stack
                .last()
                .map(|(_, id, _)| *id)
                .or_else(|| heading_stack.last().map(|(_, id, _)| *id));
            let inherited = list_stack
                .last()
                .map(|(_, _, f)| *f)
                .unwrap_or_else(|| heading_stack.last().is_some_and(|(_, _, f)| *f));
            let (id, force_task) = add_item(
                &mut items,
                &mut memberships,
                &mut position,
                &item_text,
                false,
                inherited,
                parent,
            );
            list_stack.push((indent, id, force_task));
            continue;
        }

        // Plain text: goes into the notes of the nearest enclosing heading, if any.
        if let Some(&(_, heading_id, _)) = heading_stack.last() {
            notes.entry(heading_id).or_default().push(trimmed.to_string());
        }
    }

    for item in items.iter_mut() {
        if let Some(lines) = notes.remove(&item.id) {
            let text = lines.join("\n").trim().to_string();
            if !text.is_empty() {
                item.notes = Some(text);
            }
        }
    }

    MarkdownImport { items, memberships }
}

/// Parse an ATX heading (`# Title` through `###### Title`); returns the level and title.
fn heading_line(trimmed: &str) -> Option<(usize, String)> {
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((hashes, rest.trim().to_string()))
}

/// Parse a bullet (`- `, `* `, `+ `) or numbered (`1. `, `1) `) list item; returns the
/// leading indent (in spaces, tabs counted as 4) and the item text.
fn list_item_line(line: &str) -> Option<(usize, String)> {
    let indent_chars = line.len() - line.trim_start().len();
    let indent: usize = line[..indent_chars]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let rest = line.trim_start();

    if let Some(text) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        return Some((indent, text.trim().to_string()));
    }

    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let after = &rest[digits.len()..];
        if let Some(text) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
            return Some((indent, text.trim().to_string()));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(items: &'a [Item], text: &str) -> &'a Item {
        items.iter().find(|i| i.text == text).unwrap()
    }

    fn parent_of(memberships: &[Membership], item_id: Uuid) -> Option<Uuid> {
        memberships
            .iter()
            .find(|m| m.item_id == item_id)
            .and_then(|m| m.parent_id)
    }

    #[test]
    fn headings_nest_and_collect_notes() {
        let md = "# Groceries\nBuy stuff this week\n\n## Produce\n- Apples\n- Bananas\n\n## Dairy\n- Milk\n";
        let import = parse_markdown(md);
        let groceries = find(&import.items, "Groceries");
        assert_eq!(groceries.notes.as_deref(), Some("Buy stuff this week"));
        assert!(groceries.is_list);

        let produce = find(&import.items, "Produce");
        assert_eq!(parent_of(&import.memberships, produce.id), Some(groceries.id));

        let apples = find(&import.items, "Apples");
        assert_eq!(parent_of(&import.memberships, apples.id), Some(produce.id));

        let dairy = find(&import.items, "Dairy");
        assert_eq!(parent_of(&import.memberships, dairy.id), Some(groceries.id));
    }

    #[test]
    fn nested_list_items_by_indent() {
        let md = "- Fruit\n  - Apples\n  - Bananas\n- Veggies\n";
        let import = parse_markdown(md);
        let fruit = find(&import.items, "Fruit");
        let apples = find(&import.items, "Apples");
        let veggies = find(&import.items, "Veggies");
        assert_eq!(parent_of(&import.memberships, apples.id), Some(fruit.id));
        assert_eq!(parent_of(&import.memberships, veggies.id), None);
    }

    #[test]
    fn explicit_todo_and_done_prefixes() {
        let md = "- TODO: Buy milk\n- DONE: Pay rent\n- Just a note\n";
        let import = parse_markdown(md);
        let milk = find(&import.items, "Buy milk");
        assert!(!milk.is_list && !milk.done);
        let rent = find(&import.items, "Pay rent");
        assert!(!rent.is_list && rent.done);
        let plain = find(&import.items, "Just a note");
        assert!(!plain.done);
    }

    #[test]
    fn bare_todo_marker_forces_descendants_to_tasks() {
        let md = "## Errands\n### TODO\n- Buy milk\n- Return package\n";
        let import = parse_markdown(md);
        let marker = find(&import.items, "TODO");
        assert!(!marker.done);
        let milk = find(&import.items, "Buy milk");
        assert!(!milk.is_list && !milk.done);
        assert_eq!(parent_of(&import.memberships, milk.id), Some(marker.id));
        let pkg = find(&import.items, "Return package");
        assert!(!pkg.is_list && !pkg.done);
    }

    #[test]
    fn bare_todo_marker_as_list_bullet() {
        let md = "- TODO:\n  - Wash car\n  - DONE: Mow lawn\n";
        let import = parse_markdown(md);
        let wash = find(&import.items, "Wash car");
        assert!(!wash.done);
        let mow = find(&import.items, "Mow lawn");
        assert!(mow.done);
    }
}
