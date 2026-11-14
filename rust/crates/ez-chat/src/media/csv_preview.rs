//! Vista previa de CSV/TSV como tabla de markdown.

use ez_core::pyvalue::{head_chars, py_strip};

use super::PREVIEW_CHARS;
use super::PREVIEW_ROWS;

/// Un CSV se previsualiza como tabla de markdown (las primeras filas).
pub(super) fn table_preview(
    text: &str,
    delimiter: Option<char>,
    mut truncated: bool,
) -> (String, bool) {
    let sample = head_chars(text, 2000);
    let allowed: Vec<char> = match delimiter {
        Some(value) => vec![value],
        None => vec![',', ';', '\t', '|'],
    };
    let delimiter = sniff(sample, &allowed).unwrap_or(',');
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (index, row) in records(text, delimiter).into_iter().enumerate() {
        if index > PREVIEW_ROWS {
            truncated = true;
            break;
        }
        rows.push(
            row.iter()
                .map(|cell| py_strip(&cell.replace('|', "\\|")).to_string())
                .collect(),
        );
    }
    if rows.is_empty() || rows[0].is_empty() {
        return (head_chars(text, PREVIEW_CHARS).to_string(), truncated);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut rows {
        row.resize(width, String::new());
    }
    let mut lines = vec![
        format!("| {} |", rows[0].join(" | ")),
        format!("|{}", "---|".repeat(width)),
    ];
    lines.extend(
        rows[1..]
            .iter()
            .map(|row| format!("| {} |", row.join(" | "))),
    );
    (lines.join("\n"), truncated)
}

/// Elige el delimitador más consistente entre los permitidos.
fn sniff(sample: &str, allowed: &[char]) -> Option<char> {
    let lines: Vec<&str> = sample.split('\n').filter(|line| !line.is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let mut best: Option<(bool, usize, usize, i32, char)> = None;
    for (priority, candidate) in allowed.iter().enumerate() {
        let counts: Vec<usize> = lines
            .iter()
            .map(|line| count_unquoted(line, *candidate))
            .collect();
        let positive = counts.iter().filter(|count| **count > 0).count();
        if positive == 0 {
            continue;
        }
        let total: usize = counts.iter().sum();
        let first_positive = counts.iter().copied().find(|count| *count > 0).unwrap_or(0);
        let consistent =
            positive == lines.len() && counts.iter().all(|count| *count == first_positive);
        let score = (consistent, positive, total, -(priority as i32), *candidate);
        if best.as_ref().is_none_or(|current| score > *current) {
            best = Some(score);
        }
    }
    best.map(|(_, _, _, _, character)| character)
}

fn count_unquoted(line: &str, delimiter: char) -> usize {
    let mut quoted = false;
    let mut count = 0usize;
    for character in line.chars() {
        if character == '"' {
            quoted = !quoted;
        } else if character == delimiter && !quoted {
            count += 1;
        }
    }
    count
}

/// `csv.reader` con comillas dobles.
fn records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        match character {
            '"' if field.is_empty() => {
                quoted = true;
                started = true;
            }
            '"' => field.push('"'),
            '\n' => {
                if started || !row.is_empty() {
                    row.push(field.clone());
                }
                rows.push(std::mem::take(&mut row));
                field.clear();
                started = false;
            }
            value if value == delimiter => {
                row.push(field.clone());
                field.clear();
                started = true;
            }
            value => {
                field.push(value);
                started = true;
            }
        }
    }
    if quoted || started || !row.is_empty() || !field.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_python_samples() {
        let cases = [
            (
                "mes,total\nenero,10\nfebrero,12\n",
                None,
                false,
                "| mes | total |\n|---|---|\n| enero | 10 |\n| febrero | 12 |",
                false,
            ),
            (
                "a,b\n1\n2,3,4\n",
                None,
                false,
                "| a | b |  |\n|---|---|---|\n| 1 |  |  |\n| 2 | 3 | 4 |",
                false,
            ),
            (
                "a,b\n\"x,y\",z\n\"con \"\"comillas\"\"\",w\n",
                None,
                false,
                "| a | b |\n|---|---|\n| x,y | z |\n| con \"comillas\" | w |",
                false,
            ),
            (
                "a;b\n1;2\n",
                None,
                false,
                "| a | b |\n|---|---|\n| 1 | 2 |",
                false,
            ),
            (
                "a\tb\n1\t2\n",
                Some('\t'),
                false,
                "| a | b |\n|---|---|\n| 1 | 2 |",
                false,
            ),
            (
                "a,b\n\n1,2\n",
                None,
                false,
                "| a | b |\n|---|---|\n|  |  |\n| 1 | 2 |",
                false,
            ),
            (
                "a|b\n1|2\n",
                None,
                false,
                "| a | b |\n|---|---|\n| 1 | 2 |",
                false,
            ),
            (
                "solo\nlinea\n",
                None,
                false,
                "| solo |\n|---|\n| linea |",
                false,
            ),
            (
                "x\n\"multi\nlinea\",y\n",
                None,
                false,
                "| x |  |\n|---|---|\n| multi\nlinea | y |",
                false,
            ),
            (
                "a,b,c\n1,2,3\n",
                None,
                true,
                "| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |",
                true,
            ),
        ];
        for (text, delimiter, truncated, expected, expected_more) in cases {
            let (out, more) = table_preview(text, delimiter, truncated);
            assert_eq!(out, expected);
            assert_eq!(more, expected_more);
        }
    }
}
