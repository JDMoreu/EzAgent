//! Port of `src/ezagent/speech_text.py` (F2-06).
//!
//! El orden de las transformaciones y los textos son los del Python. Los
//! cardinales y ordinales son los de `num2words(lang="es")`; solo se portan
//! las formas que usan los fixtures (hasta millones y hasta 10^15).

use std::sync::OnceLock;

use fancy_regex::{Captures, Regex};

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

const MONTHS: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];

const DIGITS: [&str; 10] = [
    "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve",
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
pattern!(thousands, r"(?<![\w.,])-?\d{1,3}(?:\.\d{3})+(?:,\d+)?(?![\w])");
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
pattern!(star_emphasis, r"(?<![\w*])\*{1,2}(\S[^*\n]*?)\*{1,2}(?![\w*])");
pattern!(spaces, r"[ \t]+");
pattern!(space_before_punctuation, r"\s+([,.;:!?])");
pattern!(multi_space, r"\s{2,}");
pattern!(ezagent, r"(?i)\bEzAgent\b");

const UNIT_WORDS: [&str; 30] = [
    "cero",
    "uno",
    "dos",
    "tres",
    "cuatro",
    "cinco",
    "seis",
    "siete",
    "ocho",
    "nueve",
    "diez",
    "once",
    "doce",
    "trece",
    "catorce",
    "quince",
    "dieciséis",
    "diecisiete",
    "dieciocho",
    "diecinueve",
    "veinte",
    "veintiuno",
    "veintidós",
    "veintitrés",
    "veinticuatro",
    "veinticinco",
    "veintiséis",
    "veintisiete",
    "veintiocho",
    "veintinueve",
];

const TENS: [&str; 7] = [
    "treinta",
    "cuarenta",
    "cincuenta",
    "sesenta",
    "setenta",
    "ochenta",
    "noventa",
];

const HUNDREDS: [&str; 9] = [
    "ciento",
    "doscientos",
    "trescientos",
    "cuatrocientos",
    "quinientos",
    "seiscientos",
    "setecientos",
    "ochocientos",
    "novecientos",
];

const ORDINAL_UNITS: [&str; 10] = [
    "primer", "segund", "tercer", "cuart", "quint", "sext", "séptim", "octav", "noven", "décim",
];

const ORDINAL_TENS: [&str; 10] = [
    "décim",
    "vigésim",
    "trigésim",
    "quadragésim",
    "quincuagésim",
    "sexagésim",
    "septuagésim",
    "octogésim",
    "nonagésim",
    "centésim",
];

const ORDINAL_HUNDREDS: [&str; 9] = [
    "centésim",
    "ducentésim",
    "tricentésim",
    "cuadrigentésim",
    "quingentésim",
    "sexcentésim",
    "septigentésim",
    "octigentésim",
    "noningentésim",
];

const ORDINAL_SCALES: [&str; 5] = [
    "milésim",
    "millonésim",
    "billonésim",
    "trillonésim",
    "cuadrillonésim",
];

fn below_thousand(value: i64) -> String {
    if value < 30 {
        return UNIT_WORDS[value as usize].to_string();
    }
    if value < 100 {
        let tens = TENS[(value / 10 - 3) as usize];
        let unit = value % 10;
        return if unit == 0 {
            tens.to_string()
        } else {
            format!("{tens} y {}", UNIT_WORDS[unit as usize])
        };
    }
    if value == 100 {
        return "cien".to_string();
    }
    let hundreds = HUNDREDS[(value / 100 - 1) as usize];
    let rest = value % 100;
    if rest == 0 {
        hundreds.to_string()
    } else {
        format!("{hundreds} {}", below_thousand(rest))
    }
}

/// `num2words(value, lang="es")` para cardinales no negativos.
fn int_words(value: i64) -> String {
    if value < 0 {
        return format!("menos {}", int_words(-value));
    }
    if value == 0 {
        return "cero".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    let millions = value / 1_000_000;
    let thousands = value / 1_000 % 1_000;
    let rest = value % 1_000;
    if millions > 0 {
        if millions == 1 {
            parts.push("un millón".to_string());
        } else {
            parts.push(format!("{} millones", int_words(millions)));
        }
    }
    if thousands > 0 {
        if thousands == 1 {
            parts.push("mil".to_string());
        } else {
            parts.push(format!("{} mil", int_words(thousands)));
        }
    }
    if rest > 0 {
        parts.push(below_thousand(rest));
    }
    parts.join(" ")
}

/// `num2words(value, lang="es", to="ordinal")`.
fn ordinal_words(value: i64) -> String {
    if value <= 0 {
        return String::new();
    }
    if value <= 10 {
        return format!("{}o", ORDINAL_UNITS[(value - 1) as usize]);
    }
    if value <= 12 {
        return format!("décimo{}", ordinal_words(value - 10));
    }
    if value <= 1_000 {
        let step = if value <= 100 {
            value / 10 * 10
        } else {
            value / 100 * 100
        };
        let stem = if value <= 100 {
            ORDINAL_TENS[(step / 10 - 1) as usize]
        } else {
            ORDINAL_HUNDREDS[(step / 100 - 1) as usize]
        };
        return format!("{stem}o {}", ordinal_words(value - step))
            .trim_end()
            .to_string();
    }
    let mut scale = 1_000_i64;
    let mut index = 0;
    while index + 1 < ORDINAL_SCALES.len() && value >= scale * 1_000 {
        scale *= 1_000;
        index += 1;
    }
    let high = value / scale;
    let prefix = if high == 1 {
        String::new()
    } else {
        int_words(high)
    };
    format!(
        "{prefix}{} {}",
        ORDINAL_SCALES[index],
        ordinal_words(value - scale * high)
    )
    .trim_end()
    .to_string()
}

fn digit_words(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|digit| digit.to_digit(10))
        .map(|digit| DIGITS[digit as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

fn fraction_words(fraction: &str) -> String {
    if fraction.starts_with('0') {
        return digit_words(fraction);
    }
    match fraction.parse::<i64>() {
        Ok(value) => int_words(value),
        // shortcut: Python usa enteros sin límite; más de 19 dígitos cae aquí.
        Err(_) => digit_words(fraction),
    }
}

fn cardinal(raw: &str) -> String {
    let negative = raw.starts_with('-');
    let digits = raw.trim_start_matches('-').replace('.', "");
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_ascii_digit()) {
        return raw.to_string();
    }
    let words = if digits.len() >= 7 {
        digit_words(&digits)
    } else {
        match digits.parse::<i64>() {
            Ok(value) => int_words(value),
            Err(_) => digit_words(&digits),
        }
    };
    if negative {
        format!("menos {words}")
    } else {
        words
    }
}

fn group<'t>(caps: &Captures<'t>, index: usize) -> &'t str {
    caps.get(index).map_or("", |capture| capture.as_str())
}

fn named<'t>(caps: &Captures<'t>, name: &str) -> &'t str {
    caps.name(name).map_or("", |capture| capture.as_str())
}

/// Compila una tabla `patrón → reemplazo` una sola vez.
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

fn integer_part(text: &str) -> i64 {
    match text.replace(['.', ','], "").parse::<i64>() {
        Ok(value) => value,
        Err(_) => 0,
    }
}

fn thousands(caps: &Captures<'_>) -> String {
    cardinal(group(caps, 0))
}

fn thousands_decimal(caps: &Captures<'_>) -> String {
    let raw = group(caps, 0);
    let negative = raw.starts_with('-');
    let body = raw.trim_start_matches('-');
    let (whole, fraction) = match body.split_once(',') {
        Some(parts) => parts,
        None => (body, ""),
    };
    let words = format!(
        "{} coma {}",
        int_words(integer_part(whole)),
        fraction_words(fraction)
    );
    if negative {
        format!("menos {words}")
    } else {
        words
    }
}

fn thousands_group(caps: &Captures<'_>) -> String {
    if group(caps, 0).contains(',') {
        thousands_decimal(caps)
    } else {
        thousands(caps)
    }
}

fn decimal_comma(caps: &Captures<'_>) -> String {
    let raw = group(caps, 0);
    let negative = raw.starts_with('-');
    let body = raw.trim_start_matches('-');
    let (whole, fraction) = body.split_once(',').unwrap_or((body, ""));
    let words = format!(
        "{} coma {}",
        int_words(integer_part(whole)),
        fraction_words(fraction)
    );
    if negative {
        format!("menos {words}")
    } else {
        words
    }
}

fn decimal_dot(caps: &Captures<'_>) -> String {
    let raw = group(caps, 0);
    let negative = raw.starts_with('-');
    let body = raw.trim_start_matches('-');
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    let words = format!(
        "{} coma {}",
        int_words(integer_part(whole)),
        fraction_words(fraction)
    );
    if negative {
        format!("menos {words}")
    } else {
        words
    }
}

fn integer(caps: &Captures<'_>) -> String {
    cardinal(group(caps, 0))
}

fn time_words(caps: &Captures<'_>) -> String {
    let hour = group(caps, 1).parse::<i64>().unwrap_or(0);
    let minute = group(caps, 2).parse::<i64>().unwrap_or(0);
    match minute {
        0 => format!("{} en punto", int_words(hour)),
        15 => format!("{} y cuarto", int_words(hour)),
        30 => format!("{} y media", int_words(hour)),
        _ => format!("{} {}", int_words(hour), int_words(minute)),
    }
}

fn date_words(caps: &Captures<'_>) -> String {
    let day = group(caps, 1).parse::<i64>().unwrap_or(0);
    let month = group(caps, 2).parse::<i64>().unwrap_or(0);
    let mut year = group(caps, 3).parse::<i64>().unwrap_or(0);
    if !(1..=12).contains(&month) {
        return group(caps, 0).to_string();
    }
    if year < 100 {
        year += 2000;
    }
    format!(
        "{} de {} de {}",
        int_words(day),
        MONTHS[(month - 1) as usize],
        int_words(year)
    )
}

fn ordinal_suffix(caps: &Captures<'_>) -> String {
    let number = group(caps, 1).parse::<i64>().unwrap_or(0);
    let suffix = group(caps, 2);
    let word = ordinal_words(number);
    if suffix == "ª" && word.ends_with('o') {
        return format!("{}a", &word[..word.len() - 1]);
    }
    word
}

fn ordinal_er(caps: &Captures<'_>) -> String {
    let number = group(caps, 1).parse::<i64>().unwrap_or(0);
    if number == 1 {
        return "primer".to_string();
    }
    ordinal_words(number)
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
    if words.ends_with("veintiuno") {
        return format!("{}veintiún", &words[..words.len() - "veintiuno".len()]);
    }
    if words == "uno" || words.ends_with(" uno") {
        return words[..words.len() - 1].to_string();
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
        .find(|(name, _)| *name == key)
        .map(|(_, names)| *names)
}

fn money_words(amount: &str, magnitude: &str, symbol: &str) -> String {
    let Some((singular, plural)) = currency(symbol) else {
        // DECISION: el Python lanzaría KeyError; el patrón solo admite monedas conocidas.
        return amount.to_string();
    };
    let (whole, mut cents) = split_amount(amount);
    let mag = magnitude.trim().to_lowercase();
    let mut number = whole.parse::<i64>().ok();
    let mut words = match number {
        Some(value) => apocope(&int_words(value)),
        None => whole.clone(),
    };
    if !mag.is_empty() && cents.is_some() && number.is_some() {
        words = format!(
            "{} coma {}",
            int_words(number.unwrap_or(0)),
            fraction_words(cents.as_deref().unwrap_or(""))
        );
        number = Some(-1);
        cents = None;
    }
    let mut text = if mag == "k" || mag == "mil" {
        let amount_words = if number == Some(1) { "" } else { &words };
        format!("{amount_words} mil {plural}")
            .replace("  ", " ")
            .trim()
            .to_string()
    } else if mag == "m" || mag == "millones" || mag == "millón" {
        if number == Some(1) {
            format!("un millón de {plural}")
        } else {
            format!("{words} millones de {plural}")
        }
    } else {
        let name = if number == Some(1) && cents.is_none() {
            singular
        } else {
            plural
        };
        format!("{words} {name}")
    };
    if let Some(decimals) = cents {
        let cents = format!("{decimals}0");
        let cents: String = cents.chars().take(2).collect();
        if cents != "00" {
            text.push_str(&format!(" con {cents}"));
        }
    }
    text
}

fn normalize_money(value: &str) -> String {
    let value = money_before().replace_all(value, |caps: &Captures<'_>| {
        money_words(named(caps, "amt"), named(caps, "mag"), named(caps, "cur"))
    });
    money_after()
        .replace_all(&value, |caps: &Captures<'_>| {
            money_words(named(caps, "amt"), named(caps, "mag"), named(caps, "cur"))
        })
        .to_string()
}

fn scale_for(word: &str) -> Option<&'static str> {
    RATES
        .iter()
        .find(|(key, _)| *key == word)
        .map(|(_, value)| *value)
}

fn normalize_rates(value: &str) -> String {
    let value = rate().replace_all(value, |caps: &Captures<'_>| {
        let key = group(caps, 1).to_lowercase();
        match scale_for(&key) {
            Some(words) => format!(" {words}"),
            None => group(caps, 0).to_string(),
        }
    });
    let value = per_time().replace_all(&value, |caps: &Captures<'_>| {
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
        .replace_all(&value, |caps: &Captures<'_>| {
            if group(caps, 1) == "h" {
                " por hora".to_string()
            } else {
                " por segundo".to_string()
            }
        })
        .to_string()
}

fn fraction(caps: &Captures<'_>) -> String {
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

fn iso_date(caps: &Captures<'_>) -> String {
    let year = group(caps, 1).parse::<i64>().unwrap_or(0);
    let month = group(caps, 2).parse::<i64>().unwrap_or(0);
    let day = group(caps, 3).parse::<i64>().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return group(caps, 0).to_string();
    }
    format!(
        "{} de {} de {}",
        int_words(day),
        MONTHS[(month - 1) as usize],
        int_words(year)
    )
}

fn phone(caps: &Captures<'_>) -> String {
    group(caps, 0)
        .split('-')
        .map(digit_words)
        .collect::<Vec<_>>()
        .join(", ")
}

fn number_range(caps: &Captures<'_>) -> String {
    let left = group(caps, 1);
    let right = group(caps, 2);
    if left.chars().count() == 3
        && right.chars().count() == 4
        && left.chars().all(|digit| digit.is_ascii_digit())
        && right.chars().all(|digit| digit.is_ascii_digit())
    {
        return group(caps, 0).to_string();
    }
    format!("{left} a {right}")
}

fn normalize_symbols(value: &str) -> String {
    let value = value
        .replace('\u{2212}', "-")
        .replace('\u{2013}', "-")
        .replace('\u{2014}', " - ");
    let value = iso_date().replace_all(&value, iso_date);
    let value = phone().replace_all(&value, phone);
    let value = number_range().replace_all(&value, number_range);
    let value = plus().replace_all(&value, "más ");
    let value = fraction().replace_all(&value, fraction);
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
        let cells: Vec<&str> = py_strip(line)
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
    let value = value.replace("°C", " grados centígrados").replace('°', " grados");
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
    let value = ordinal_er().replace_all(&value, ordinal_er);
    let value = thousands().replace_all(&value, thousands_group);
    let value = decimal_comma().replace_all(&value, decimal_comma);
    let value = decimal_dot().replace_all(&value, decimal_dot);
    integer().replace_all(&value, integer).to_string()
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
