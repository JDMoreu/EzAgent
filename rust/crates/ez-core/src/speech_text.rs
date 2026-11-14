//! Port of `src/ezagent/speech_text.py` (F2-06).
//!
//! El orden de las transformaciones y los textos son los del Python. El
//! trozo de `num2words` vive en `speech_text/numbers.rs`.

use std::sync::OnceLock;

use fancy_regex::{Captures, Regex};

pub(crate) type Caps<'t> = Captures<'t, str>;

mod numbers;

use self::numbers::{
    cardinal, date_words, decimal_words, digit_words, fraction_words, int_words, iso_date_spoken,
    ordinal_er_spoken, ordinal_suffix, time_words,
};

macro_rules! pattern {
    ($name:ident, $source:expr) => {
        fn $name() -> &'static Regex {
            static SLOT: OnceLock<Regex> = OnceLock::new();
            SLOT.get_or_init(|| Regex::new($source).expect("patrón interno válido"))
        }
    };
}

const ABBREVIATIONS: &[(&str, &str)] = &[
    (r"\bEE\.\s*UU\.", "Estados Unidos"),
    (r"\bSrta\.", "señorita"),
    (r"\bSra\.", "señora"),
    (r"\bSr\.", "señor"),
    (r"\bDra\.", "doctora"),
    (r"\bDr\.", "doctor"),
    (r"\bp\.\s*ej\.", "por ejemplo"),
    (r"\betc\.", "etcétera"),
    (r"\baprox\.", "aproximadamente"),
    (r"\bn\.\s*[ºo]", "número"),
    (r"\bN\.\s*[ºo]", "número"),
    (r"\bnúm\.", "número"),
    (r"\bpág\.", "página"),
    (r"\btel\.", "teléfono"),
    (r"\ba\.\s*C\.", "antes de Cristo"),
    (r"\bd\.\s*C\.", "después de Cristo"),
    (r"\bUd\.", "usted"),
    (r"\bUds\.", "ustedes"),
    (r"\bAv\.", "avenida"),
    (r"\bC/", "calle "),
];

const UNITS: &[(&str, &str)] = &[
    (r"(?<=\d)\s*km/h\b", " kilómetros por hora"),
    (r"(?<=\d)\s*m/s\b", " metros por segundo"),
    (r"(?<=\d)\s*°C\b", " grados centígrados"),
    (r"(?<=\d)\s*km\b", " kilómetros"),
    (r"(?<=\d)\s*kg\b", " kilogramos"),
    (r"(?<=\d)\s*cm\b", " centímetros"),
    (r"(?<=\d)\s*mm\b", " milímetros"),
    (r"(?<=\d)\s*ml\b", " mililitros"),
    (r"(?<=\d)\s*GB\b", " gigabytes"),
    (r"(?<=\d)\s*MB\b", " megabytes"),
    (r"(?<=\d)\s*ms\b", " milisegundos"),
    (r"(?<=\d)\s*s\b", " segundos"),
    (r"\bkm/h\b", "kilómetros por hora"),
    (r"\bkm\b", "kilómetros"),
    (r"\bkg\b", "kilogramos"),
    (r"\bcm\b", "centímetros"),
    (r"\bmm\b", "milímetros"),
    (r"\bml\b", "mililitros"),
    (r"\bGB\b", "gigabytes"),
    (r"\bMB\b", "megabytes"),
    (r"\bms\b", "milisegundos"),
];

const CURRENCIES: &[(&str, (&str, &str))] = &[
    ("$", ("dólar", "dólares")),
    ("US$", ("dólar", "dólares")),
    ("USD", ("dólar", "dólares")),
    ("€", ("euro", "euros")),
    ("EUR", ("euro", "euros")),
    ("£", ("libra", "libras")),
    ("MXN", ("peso mexicano", "pesos mexicanos")),
    ("COP", ("peso colombiano", "pesos colombianos")),
    ("ARS", ("peso argentino", "pesos argentinos")),
    ("CLP", ("peso chileno", "pesos chilenos")),
    ("PEN", ("sol", "soles")),
    ("BRL", ("real", "reales")),
];

const RATES: &[(&str, &str)] = &[
    ("mes", "al mes"),
    ("mo", "al mes"),
    ("month", "al mes"),
    ("año", "al año"),
    ("anio", "al año"),
    ("yr", "al año"),
    ("year", "al año"),
    ("día", "al día"),
    ("dia", "al día"),
    ("day", "al día"),
    ("semana", "a la semana"),
    ("week", "a la semana"),
    ("trimestre", "al trimestre"),
    ("hora", "por hora"),
    ("hr", "por hora"),
    ("minuto", "por minuto"),
    ("min", "por minuto"),
    ("segundo", "por segundo"),
    ("usuario", "por usuario"),
    ("user", "por usuario"),
    ("persona", "por persona"),
    ("licencia", "por licencia"),
    ("asiento", "por asiento"),
    ("seat", "por asiento"),
    ("dispositivo", "por dispositivo"),
    ("token", "por token"),
    ("tokens", "por token"),
    ("mensaje", "por mensaje"),
    ("llamada", "por llamada"),
];

const FRACTIONS: &[(i64, (&str, &str))] = &[
    (2, ("medio", "medios")),
    (3, ("tercio", "tercios")),
    (4, ("cuarto", "cuartos")),
    (5, ("quinto", "quintos")),
    (8, ("octavo", "octavos")),
    (10, ("décimo", "décimos")),
];

const PER_TIME_UNITS: &[(&str, &str)] = &[
    ("KB", "kilobytes"),
    ("MB", "megabytes"),
    ("GB", "gigabytes"),
    ("TB", "terabytes"),
];

pattern!(image, r"!\[([^\]]*)\]\([^)]*\)");
pattern!(link, r"\[([^\]]+)\]\([^)]*\)");
pattern!(url, r"https?://\S+");
pattern!(table_separator, r"\A\s*\|?[\s:|-]+\|?\s*\z");
pattern!(heading, r"(?m)^\s*#{1,6}\s*");
pattern!(bullet, r"(?m)^\s*[-*•]\s+");
pattern!(ordered, r"(?m)^\s*\d+[.)]\s+");
pattern!(
    emoji,
    "[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}\u{1F1E6}-\u{1F1FF}\u{2190}-\u{21FF}\u{FE0F}\u{200D}]+"
);
pattern!(
    money_before,
    r"(?P<cur>US\$|\$|€|£)\s?(?P<amt>\d{1,3}(?:[.,]\d{3})+(?:[.,]\d{1,2})?|\d+(?:[.,]\d{1,2})?)(?P<mag>(?:\s?(?:k|K|M|mil|millones|millón))?)(?![\w])"
);
pattern!(
    money_after,
    r"(?<![\w.,])(?P<amt>\d{1,3}(?:[.,]\d{3})+(?:[.,]\d{1,2})?|\d+(?:[.,]\d{1,2})?)(?P<mag>(?:\s?(?:k|K|M|mil|millones|millón))?)\s?(?P<cur>USD|EUR|MXN|COP|ARS|CLP|PEN|BRL|US\$|\$|€|£)(?![A-Za-z])"
);
pattern!(
    rate,
    r"(?i)(?<=[\w%)])\s*/\s*(dispositivo|trimestre|licencia|segundo|usuario|persona|asiento|mensaje|llamada|semana|minuto|tokens|month|token|anio|year|week|hora|user|seat|mes|año|día|dia|day|min|mo|yr|hr)\b"
);
pattern!(
    per_time,
    r"\b(KB|MB|GB|TB|bytes?|bits?|tokens?|req|requests?|peticiones|mensajes|palabras|caracteres|líneas|filas|frames?)\s*/\s*([sh])\b"
);
pattern!(rate_short, r"(?<=\d)\s*/\s*([hs])\b");
pattern!(date, r"\b(\d{1,2})/(\d{1,2})/(\d{2,4})\b");
pattern!(time, r"\b(\d{1,2}):(\d{2})\b");
pattern!(ordinal, r"\b(\d+)\s*([ºª])(?=\W|$)");
pattern!(ordinal_er, r"\b(\d+)\s*er\b");
pattern!(
    thousands,
    r"(?<![\w.,])-?\d{1,3}(?:\.\d{3})+(?:,\d+)?(?![\w])"
);
pattern!(decimal_comma, r"(?<![\w.,])-?\d+,\d+(?![\w])");
pattern!(decimal_dot, r"(?<![\w.,])-?\d+\.\d{1,3}(?![\w])");
pattern!(integer, r"(?<![\w.,])-?\d+(?![\w])");
pattern!(iso_date, r"\b(\d{4})-(\d{2})-(\d{2})\b");
pattern!(
    phone,
    r"(?<![\w.,/-])(?:\d{3}-\d{4}|\d{2,4}(?:-\d{2,4}){2,3})(?![\w/-])"
);
pattern!(
    number_range,
    r"(?<![\w.,/-])(\d+(?:[.,]\d+)?)\s*-\s*(\d+(?:[.,]\d+)?)(?![\w/-])"
);
pattern!(plus, r"(?<![\w+])\+(?=\d)");
pattern!(fraction, r"(?<![\w/\d])(\d{1,3})/(\d{1,3})(?![\w/])");
pattern!(y_over_o, r"(?i)\by\s?/\s?o\b");
pattern!(
    slash_letters,
    r"(?<=[A-Za-zÁÉÍÓÚÜáéíóúüñÑ])\s?/\s?(?=[A-Za-zÁÉÍÓÚÜáéíóúüñÑ])"
);
pattern!(spaced_hyphen, r"\s-\s");
pattern!(tilde_digit, r"~(?=\d)");
pattern!(hash_digit, r"#(?=\d)");
pattern!(k_suffix, r"\b(\d+(?:[.,]\d+)?)\s?[kK]\b");
pattern!(
    star_emphasis,
    r"(?<![\w*])\*{1,2}(\S[^*\n]*?)\*{1,2}(?![\w*])"
);
pattern!(spaces, r"[ \t]+");
pattern!(space_before_punctuation, r"\s+([,.;:!?])");
pattern!(multi_space, r"\s{2,}");
pattern!(ezagent, r"(?i)\bEzAgent\b");

pub(crate) fn group<'t>(caps: &Caps<'t>, index: usize) -> &'t str {
    caps.get(index).map_or("", |capture| capture.as_str())
}

fn named<'t>(caps: &Caps<'t>, name: &str) -> &'t str {
    caps.name(name).map_or("", |capture| capture.as_str())
}

fn compile_table(table: &'static [(&'static str, &'static str)]) -> Vec<(&'static str, Regex)> {
    table
        .iter()
        .map(|(source, replacement)| {
            (
                *replacement,
                Regex::new(source).expect("patrón interno válido"),
            )
        })
        .collect()
}

fn abbreviations() -> &'static [(&'static str, Regex)] {
    static SLOTS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    SLOTS.get_or_init(|| compile_table(ABBREVIATIONS))
}

fn units() -> &'static [(&'static str, Regex)] {
    static SLOTS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    SLOTS.get_or_init(|| compile_table(UNITS))
}

fn split_amount(amount: &str) -> (String, Option<String>) {
    let dot = amount.rfind('.');
    let comma = amount.rfind(',');
    let decimal = if dot.is_some() && comma.is_some() {
        Some(if dot > comma { '.' } else { ',' })
    } else if dot.is_some() || comma.is_some() {
        let separator = if dot.is_some() { '.' } else { ',' };
        let parts: Vec<&str> = amount.split(separator).collect();
        if parts.len() == 2 && (1..=2).contains(&parts[1].chars().count()) {
            Some(separator)
        } else {
            None
        }
    } else {
        None
    };
    match decimal {
        Some(separator) => {
            let (whole, fraction) = amount.rsplit_once(separator).unwrap_or((amount, ""));
            (whole.replace(['.', ','], ""), Some(fraction.to_string()))
        }
        None => (amount.replace(['.', ','], ""), None),
    }
}

fn apocope(words: &str) -> String {
    if let Some(stem) = words.strip_suffix("veintiuno") {
        return format!("{stem}veintiún");
    }
    if words == "uno" || words.ends_with(" uno") {
        return words.strip_suffix('o').unwrap_or(words).to_string();
    }
    words.to_string()
}

fn currency(symbol: &str) -> Option<(&'static str, &'static str)> {
    let key = if !symbol.is_empty() && symbol.chars().all(char::is_alphabetic) {
        symbol.to_uppercase()
    } else {
        symbol.to_string()
    };
    CURRENCIES
        .iter()
        .find(|(name, _)| *name == key.as_str())
        .map(|(_, names)| *names)
}

fn money_words(amount: &str, magnitude: &str, symbol: &str) -> String {
    let Some((singular, plural)) = currency(symbol) else {
        // DECISION: el Python lanzaría KeyError; el patrón solo deja pasar monedas conocidas.
        return amount.to_string();
    };
    let (whole, mut cents) = split_amount(amount);
    let mag = magnitude.trim().to_lowercase();
    let mut number = whole.parse::<i64>().ok();
    let mut words = match number {
        Some(value) => apocope(&int_words(value)),
        None => whole.clone(),
    };
    if !mag.is_empty() {
        if let (Some(value), Some(decimals)) = (number, cents.as_deref()) {
            words = format!("{} coma {}", int_words(value), fraction_words(decimals));
            number = Some(-1);
            cents = None;
        }
    }
    let mut text = if mag == "k" || mag == "mil" {
        let count = if number == Some(1) {
            ""
        } else {
            words.as_str()
        };
        format!("{count} mil {plural}")
            .replace("  ", " ")
            .trim()
            .to_string()
    } else if mag == "m" || mag == "millones" || mag == "millón" {
        if number == Some(1) {
            format!("un millón de {plural}")
        } else {
            format!("{words} millones de {plural}")
        }
    } else if number == Some(1) && cents.is_none() {
        format!("{words} {singular}")
    } else {
        format!("{words} {plural}")
    };
    if let Some(decimals) = cents {
        let padded: String = format!("{decimals}0").chars().take(2).collect();
        if padded != "00" {
            text.push_str(&format!(" con {padded}"));
        }
    }
    text
}

fn normalize_money(value: &str) -> String {
    let amount_words =
        |caps: &Caps<'_>| money_words(named(caps, "amt"), named(caps, "mag"), named(caps, "cur"));
    let value = money_before().replace_all(value, amount_words);
    money_after().replace_all(&value, amount_words).to_string()
}

fn normalize_rates(value: &str) -> String {
    let value = rate().replace_all(value, |caps: &Caps<'_>| {
        let key = group(caps, 1).to_lowercase();
        match RATES.iter().find(|(name, _)| *name == key.as_str()) {
            Some((_, words)) => format!(" {words}"),
            None => group(caps, 0).to_string(),
        }
    });
    let value = per_time().replace_all(&value, |caps: &Caps<'_>| {
        let unit = group(caps, 1);
        let name = PER_TIME_UNITS
            .iter()
            .find(|(key, _)| *key == unit)
            .map_or(unit, |(_, words)| *words);
        let period = if group(caps, 2) == "h" {
            "hora"
        } else {
            "segundo"
        };
        format!("{name} por {period}")
    });
    rate_short()
        .replace_all(&value, |caps: &Caps<'_>| {
            if group(caps, 1) == "h" {
                " por hora".to_string()
            } else {
                " por segundo".to_string()
            }
        })
        .to_string()
}

fn fraction_spoken(caps: &Caps<'_>) -> String {
    let numerator = group(caps, 1).parse::<i64>().unwrap_or(0);
    let denominator = group(caps, 2).parse::<i64>().unwrap_or(0);
    if (numerator, denominator) == (24, 7) {
        return "todos los días, a todas horas".to_string();
    }
    if denominator == 0 {
        return group(caps, 0).to_string();
    }
    let names = FRACTIONS
        .iter()
        .find(|(value, _)| *value == denominator)
        .map(|(_, names)| *names);
    if let Some((singular, plural)) = names {
        if numerator > 0 && numerator < denominator {
            let word = if numerator == 1 { singular } else { plural };
            let head = if numerator == 1 {
                "un".to_string()
            } else {
                int_words(numerator)
            };
            return format!("{head} {word}");
        }
    }
    format!("{} entre {}", group(caps, 1), group(caps, 2))
}

fn phone_spoken(caps: &Caps<'_>) -> String {
    group(caps, 0)
        .split('-')
        .map(digit_words)
        .collect::<Vec<_>>()
        .join(", ")
}

fn number_range_spoken(caps: &Caps<'_>) -> String {
    let left = group(caps, 1);
    let right = group(caps, 2);
    let is_digits =
        |text: &str| !text.is_empty() && text.chars().all(|digit| digit.is_ascii_digit());
    if left.chars().count() == 3
        && right.chars().count() == 4
        && is_digits(left)
        && is_digits(right)
    {
        return group(caps, 0).to_string();
    }
    format!("{left} a {right}")
}

fn normalize_symbols(value: &str) -> String {
    let value = value
        .replace(['\u{2212}', '\u{2013}'], "-")
        .replace('\u{2014}', " - ");
    let value = iso_date().replace_all(&value, iso_date_spoken);
    let value = phone().replace_all(&value, phone_spoken);
    let value = number_range().replace_all(&value, number_range_spoken);
    let value = plus().replace_all(&value, "más ");
    let value = fraction().replace_all(&value, fraction_spoken);
    let value = y_over_o().replace_all(&value, "y o");
    let value = slash_letters().replace_all(&value, " o ");
    let value = spaced_hyphen().replace_all(&value, ", ");
    let mut value = value.to_string();
    for (symbol, words) in [
        ("×", " por "),
        ("≈", " aproximadamente "),
        ("≥", " mayor o igual que "),
        ("≤", " menor o igual que "),
        ("…", "..."),
        (" = ", " igual a "),
        ("~~", ""),
    ] {
        value = value.replace(symbol, words);
    }
    let value = tilde_digit().replace_all(&value, "aproximadamente ");
    let value = hash_digit().replace_all(&value, "número ");
    let value = k_suffix().replace_all(&value, "$1 mil");
    let value = star_emphasis().replace_all(&value, "$1");
    value.replace('*', " ")
}

fn flatten_tables(value: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in value.split('\n') {
        if line.matches('|').count() < 2 {
            lines.push(line.to_string());
            continue;
        }
        if table_separator().is_match(line).unwrap_or(false) && line.contains('-') {
            continue;
        }
        let cells: Vec<String> = py_strip(line)
            .trim_matches('|')
            .split('|')
            .map(py_strip)
            .filter(|cell| !cell.is_empty())
            .collect();
        let text = cells.join(", ");
        if text.ends_with('.') || text.ends_with('!') || text.ends_with('?') {
            lines.push(text);
        } else {
            lines.push(format!("{text}."));
        }
    }
    lines.join("\n")
}

fn clean_markdown(value: &str) -> String {
    let value = image().replace_all(value, "$1");
    let value = link().replace_all(&value, "$1");
    let value = url().replace_all(&value, "");
    let value = flatten_tables(&value);
    let value = heading().replace_all(&value, "");
    let value = bullet().replace_all(&value, "");
    let value = ordered().replace_all(&value, "");
    let value = value.replace('`', "").replace("**", "").replace("__", "");
    emoji().replace_all(&value, "").to_string()
}

fn money_and_symbols(value: &str) -> String {
    let value = normalize_money(value);
    let value = normalize_rates(&value);
    let value = value.replace('&', " y ").replace('%', " por ciento");
    let value = value
        .replace('€', " euros")
        .replace('£', " libras")
        .replace('$', " dólares");
    let value = value
        .replace("°C", " grados centígrados")
        .replace('°', " grados");
    let mut value = value.replace(" y/o ", " y o ");
    for (replacement, regex) in abbreviations() {
        value = regex.replace_all(&value, *replacement).to_string();
    }
    for (replacement, regex) in units() {
        value = regex.replace_all(&value, *replacement).to_string();
    }
    value
}

fn normalize_numbers(value: &str) -> String {
    let value = date().replace_all(value, date_words);
    let value = time().replace_all(&value, time_words);
    let value = ordinal().replace_all(&value, ordinal_suffix);
    let value = ordinal_er().replace_all(&value, ordinal_er_spoken);
    let value = thousands().replace_all(&value, |caps: &Caps<'_>| {
        let raw = group(caps, 0);
        if raw.contains(',') {
            decimal_words(raw, ',')
        } else {
            cardinal(raw)
        }
    });
    let value =
        decimal_comma().replace_all(&value, |caps: &Caps<'_>| decimal_words(group(caps, 0), ','));
    let value =
        decimal_dot().replace_all(&value, |caps: &Caps<'_>| decimal_words(group(caps, 0), '.'));
    integer()
        .replace_all(&value, |caps: &Caps<'_>| cardinal(group(caps, 0)))
        .to_string()
}

fn collapse_whitespace(value: &str) -> String {
    let value = spaces().replace_all(value, " ");
    let value = space_before_punctuation().replace_all(&value, "$1");
    py_strip(&multi_space().replace_all(&value, " "))
}

fn is_py_space(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

fn py_strip(value: &str) -> String {
    value.trim_matches(is_py_space).to_string()
}

/// Texto listo para el sintetizador (equivalente a `normalize_for_speech`).
///
/// DECISION: el `_text_type` del Python no se lee nunca, así que no se porta.
pub fn normalize_for_speech(text: &str) -> String {
    let value = py_strip(text);
    if value.is_empty() {
        return value;
    }
    let value = clean_markdown(&value);
    let value = money_and_symbols(&value);
    let value = normalize_symbols(&value);
    let value = normalize_numbers(&value);
    collapse_whitespace(&value)
}

/// Igual que `normalize_for_speech` y además fija la marca «Es Agente».
pub fn normalize_for_kokoro(text: &str) -> String {
    let value = normalize_for_speech(text);
    ezagent().replace_all(&value, "Es Agente").to_string()
}
