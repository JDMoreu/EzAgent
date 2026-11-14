//! Port of `num2words(value, lang="es")` used by `speech_text.rs` (F2-06).

const DIGITS: [&str; 10] = [
    "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve",
];

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

/// `num2words(value, lang="es")` para cardinales.
pub(crate) fn int_words(value: i64) -> String {
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
pub(crate) fn ordinal_words(value: i64) -> String {
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

/// Un teléfono se dicta cifra a cifra.
pub(crate) fn digit_words(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|digit| digit.to_digit(10))
        .map(|digit| DIGITS[digit as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn fraction_words(fraction: &str) -> String {
    if fraction.starts_with('0') {
        return digit_words(fraction);
    }
    match fraction.parse::<i64>() {
        Ok(value) => int_words(value),
        // shortcut: Python usa enteros sin límite; más de 19 dígitos cae aquí.
        Err(_) => digit_words(fraction),
    }
}

/// `_cardinal` del Python: números de 7 cifras o más se dicen dígito a dígito.
pub(crate) fn cardinal(raw: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinals_match_num2words() {
        assert_eq!(int_words(0), "cero");
        assert_eq!(int_words(21), "veintiuno");
        assert_eq!(int_words(100), "cien");
        assert_eq!(int_words(101), "ciento uno");
        assert_eq!(int_words(1_000), "mil");
        assert_eq!(int_words(2_026), "dos mil veintiséis");
        assert_eq!(int_words(21_000), "veintiuno mil");
        assert_eq!(int_words(1_234_567), "un millón doscientos treinta y cuatro mil quinientos sesenta y siete");
        assert_eq!(int_words(1_000_000_000), "mil millones");
    }

    #[test]
    fn ordinals_match_num2words() {
        assert_eq!(ordinal_words(1), "primero");
        assert_eq!(ordinal_words(3), "tercero");
        assert_eq!(ordinal_words(10), "décimo");
        assert_eq!(ordinal_words(11), "décimoprimero");
        assert_eq!(ordinal_words(13), "décimo tercero");
        assert_eq!(ordinal_words(20), "vigésimo");
        assert_eq!(ordinal_words(21), "vigésimo primero");
        assert_eq!(ordinal_words(1_000), "milésimo");
    }

    #[test]
    fn digit_forms_keep_python_quirks() {
        assert_eq!(cardinal("1000000"), "uno cero cero cero cero cero cero");
        assert_eq!(cardinal("-7"), "menos siete");
        assert_eq!(fraction_words("0001"), "cero cero cero uno");
        assert_eq!(fraction_words("234"), "doscientos treinta y cuatro");
    }
}
