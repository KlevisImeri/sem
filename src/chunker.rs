use tree_sitter::{Node, Parser};

#[derive(Debug, Clone)]
pub struct Chunk {
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub content: String,
}

const TOP_LEVEL_ITEMS: &[&str] = &[
    "function_item",
    "async_function_item",
    "struct_item",
    "enum_item",
    "impl_item",
    "trait_item",
    "mod_item",
    "type_item",
    "const_item",
    "static_item",
    "extern_crate_declaration",
    "macro_definition",
];

const CONTAINER_ITEMS: &[&str] = &["mod_item", "trait_item", "impl_item"];

const MAX_CHUNK_CHARS: usize = 5000;
const MIN_CHUNK_CHARS: usize = 50;

#[derive(Clone)]
struct WrapInfo {
    header: String,
    body_indent: String,
    closing_indent: String,
}

pub fn chunk_file(file_path: &str, code: &[u8]) -> Result<Vec<Chunk>, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|e| format!("Failed to load Rust grammar: {e}"))?;

    let tree = parser
        .parse(code, None)
        .ok_or("Failed to parse code")?;
    let root = tree.root_node();

    let mut chunks = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if TOP_LEVEL_ITEMS.contains(&child.kind()) {
            process_node(&child, code, file_path, &[], true, true, &mut chunks);
        }
    }

    Ok(chunks)
}

fn process_node(
    node: &Node,
    code: &[u8],
    file_path: &str,
    wrappers: &[WrapInfo],
    is_first: bool,
    is_last: bool,
    chunks: &mut Vec<Chunk>,
) {
    let kind = node.kind();

    if CONTAINER_ITEMS.contains(&kind) {
        let text = extract_node_text(node, code);
        let wrapper = WrapInfo {
            header: extract_container_header(node, &text),
            body_indent: format!("{}    ", detect_indent(&text)),
            closing_indent: detect_indent(&text).to_string(),
        };
        let mut new_wrappers = wrappers.to_vec();
        new_wrappers.push(wrapper);

        let children: Vec<_> = declaration_list_children(node)
            .into_iter()
            .filter(|c| TOP_LEVEL_ITEMS.contains(&c.kind()))
            .collect();

        let count = children.len();
        for (i, child) in children.into_iter().enumerate() {
            process_node(&child, code, file_path, &new_wrappers, i == 0, i == count - 1, chunks);
        }
        return;
    }

    if !TOP_LEVEL_ITEMS.contains(&kind) {
        return;
    }

    let text = extract_node_text(node, code);
    if text.len() < MIN_CHUNK_CHARS {
        return;
    }
    let line_start = node.start_position().row + 1;
    let line_end = node.end_position().row + 1;

    if text.len() <= MAX_CHUNK_CHARS {
        let wrapped = build_wrapped_chunk(&text, wrappers, is_first, is_last);
        chunks.push(Chunk {
            file_path: file_path.to_string(),
            line_start,
            line_end,
            content: wrapped,
        });
        return;
    }

    let sub_chunks = split_large_content(&text, wrappers, line_start, line_end);
    for mut chunk in sub_chunks {
        chunk.file_path = file_path.to_string();
        chunks.push(chunk);
    }
}

fn build_wrapped_chunk(
    content: &str,
    wrappers: &[WrapInfo],
    is_first: bool,
    is_last: bool,
) -> String {
    if wrappers.is_empty() {
        return content.to_string();
    }

    let mut result = String::new();

    for (i, wrapper) in wrappers.iter().enumerate() {
        result.push_str(&wrapper.header);
        let is_innermost = i == wrappers.len() - 1;
        if !is_innermost {
            result.push_str(&wrapper.body_indent);
            result.push_str("...\n");
        } else if !is_first {
            result.push_str(&wrapper.body_indent);
            result.push_str("...\n");
        }
    }

    if let Some(innermost) = wrappers.last() {
        result.push_str(&innermost.body_indent);
    }
    result.push_str(content);

    if !result.ends_with('\n') {
        result.push('\n');
    }

    for (i, wrapper) in wrappers.iter().rev().enumerate() {
        let is_innermost = i == 0;
        if !is_innermost || !is_last {
            result.push_str(&wrapper.body_indent);
            result.push_str("...\n");
        }
        result.push_str(&wrapper.closing_indent);
        result.push_str("}\n");
    }

    result
}

fn split_large_content(
    content: &str,
    wrappers: &[WrapInfo],
    line_start: usize,
    line_end: usize,
) -> Vec<Chunk> {
    let lines: Vec<&str> = content.lines().collect();

    let sig_end_idx = lines
        .iter()
        .position(|l| l.contains('{'))
        .unwrap_or(0)
        + 1;

    let sig_lines = &lines[..sig_end_idx];
    let body_lines = if lines.len() > sig_end_idx + 1 {
        &lines[sig_end_idx..lines.len() - 1]
    } else {
        &[]
    };

    let sig_text: String = sig_lines.iter().map(|l| format!("{l}\n")).collect();
    let dummy = build_wrapped_chunk("X", wrappers, true, true);
    let wrapper_overhead = dummy.len().saturating_sub(1);
    let body_budget = MAX_CHUNK_CHARS.saturating_sub(wrapper_overhead + sig_text.len() + 8);

    let parts = split_lines_into_parts(body_lines, body_budget);

    if parts.len() <= 1 {
        let wrapped = build_wrapped_chunk(content, wrappers, true, true);
        return vec![Chunk {
            file_path: String::new(),
            line_start,
            line_end,
            content: wrapped,
        }];
    }

    let body_indent = detect_indent_from_lines(body_lines);
    let dots = format!("{body_indent}...\n");

    let mut chunks = Vec::new();
    let mut body_line_offset = 0;

    for (i, part) in parts.iter().enumerate() {
        let is_first_part = i == 0;
        let is_last_part = i == parts.len() - 1;

        let mut part_content = String::new();
        for line in sig_lines {
            part_content.push_str(line);
            part_content.push('\n');
        }
        if !is_first_part {
            part_content.push_str(&dots);
        }
        for line in part {
            part_content.push_str(line);
            part_content.push('\n');
        }
        if !is_last_part {
            part_content.push_str(&dots);
        }

        let wrapped = build_wrapped_chunk(&part_content, wrappers, true, true);

        let part_start_line = sig_end_idx + body_line_offset;
        let part_line_start = if is_first_part {
            line_start
        } else {
            line_start + part_start_line
        };
        let part_line_end = if is_last_part {
            line_end
        } else {
            line_start + part_start_line + part.len()
        };

        chunks.push(Chunk {
            file_path: String::new(),
            line_start: part_line_start,
            line_end: part_line_end,
            content: wrapped,
        });

        body_line_offset += part.len();
    }

    chunks
}

fn extract_container_header(node: &Node, text: &str) -> String {
    let children = declaration_list_children(node);

    let mut header_end = 0;
    for child in &children {
        if TOP_LEVEL_ITEMS.contains(&child.kind()) {
            header_end = child.start_byte() - node.start_byte();
            break;
        }
    }

    if header_end == 0 {
        return format!("{}\n", text.trim_end());
    }

    let header = text[..header_end].trim_end();
    format!("{}\n", header)
}

fn extract_node_text<'a>(node: &Node, code: &'a [u8]) -> String {
    String::from_utf8_lossy(&code[node.start_byte()..node.end_byte()]).to_string()
}

fn declaration_list_children<'a>(impl_node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = impl_node.walk();
    for child in impl_node.children(&mut cursor) {
        if child.kind() == "declaration_list" {
            let mut inner = child.walk();
            return child.children(&mut inner).collect();
        }
    }
    vec![]
}

fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() {
            let indent_len = line.len() - trimmed.len();
            return " ".repeat(indent_len);
        }
    }
    String::new()
}

fn detect_indent_from_lines(lines: &[&str]) -> String {
    for line in lines {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() {
            let indent_len = line.len() - trimmed.len();
            return " ".repeat(indent_len);
        }
    }
    String::new()
}

fn split_lines_into_parts<'a>(lines: &'a [&'a str], max_chars: usize) -> Vec<Vec<&'a str>> {
    if max_chars == 0 || lines.is_empty() {
        return if lines.is_empty() {
            vec![]
        } else {
            vec![lines.to_vec()]
        };
    }

    let mut parts = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut current_len: usize = 0;

    let mut i = 0;
    while i < lines.len() {
        let line_len = lines[i].len() + 1;

        if current_len + line_len > max_chars && !current.is_empty() {
            let split_at = find_blank_line_boundary(&current, &lines[i..], max_chars);
            if split_at > 0 && split_at < current.len() {
                let keep = current.split_off(split_at);
                current_len = keep.iter().map(|l| l.len() + 1).sum();
                parts.push(current);
                current = keep;
            } else {
                parts.push(current);
                current = Vec::new();
                current_len = 0;
            }
            continue;
        }

        current.push(lines[i]);
        current_len += line_len;
        i += 1;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn find_blank_line_boundary(current: &[&str], remaining: &[&str], max_chars: usize) -> usize {
    let mut best = current.len();

    for idx in (0..current.len()).rev() {
        if current[idx].trim().is_empty() {
            let candidate_len: usize = current[..idx].iter().map(|l| l.len() + 1).sum();
            if candidate_len <= max_chars {
                best = idx + 1;
                break;
            }
        }
    }

    if best == current.len() && !remaining.is_empty() {
        if let Some(next_blank) = remaining.iter().position(|l| l.trim().is_empty()) {
            let extend_to = next_blank + 1;
            let combined: Vec<&str> = current
                .iter()
                .chain(remaining[..extend_to.min(remaining.len())].iter())
                .copied()
                .collect();
            let combined_len: usize = combined.iter().map(|l| l.len() + 1).sum();
            if combined_len <= max_chars {
                best = combined.len();
            }
        }
    }

    best
}
