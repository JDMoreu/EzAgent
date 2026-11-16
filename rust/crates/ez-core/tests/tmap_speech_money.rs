//! Casos de `test_speech_money.py` con las entradas exactas del Python (TMAP-4).

use ez_core::speech_text::normalize_for_speech;

fn check(cases: &[(&str, &str)]) {
    for (raw, expected) in cases {
        assert_eq!(&normalize_for_speech(raw), expected, "entrada: {raw:?}");
    }
}

#[test]
fn prices_and_rates_are_spoken_not_spelled() {
    check(&[
        ("Cuesta $20/mes.", "Cuesta veinte dólares al mes."),
        (
            "El plan anual: $200/año",
            "El plan anual: doscientos dólares al año",
        ),
        (
            "Son US$ 15 por usuario/mes",
            "Son quince dólares por usuario al mes",
        ),
        ("20 USD/mes", "veinte dólares al mes"),
        ("€9.99/mes", "nueve euros con noventa y nueve al mes"),
        ("£5 a la semana", "cinco libras a la semana"),
        (
            "$3/día y $2/hora",
            "tres dólares al día y dos dólares por hora",
        ),
        ("$12 por usuario/mes", "doce dólares por usuario al mes"),
    ]);
}

#[test]
fn amounts_agree_in_number_and_read_separators_correctly() {
    check(&[
        ("$1", "un dólar"),
        ("$21", "veintiún dólares"),
        ("$31 y $101", "treinta y un dólares y ciento un dólares"),
        ("1 USD", "un dólar"),
        ("$1,200", "mil doscientos dólares"),
        ("$1.200", "mil doscientos dólares"),
        ("$1,200.50", "mil doscientos dólares con cincuenta"),
        ("$1.200,50", "mil doscientos dólares con cincuenta"),
        ("$20.5", "veinte dólares con cincuenta"),
        ("$20.00", "veinte dólares"),
        ("$1M", "un millón de dólares"),
        ("$2M", "dos millones de dólares"),
        ("€2.5M", "dos coma cinco millones de euros"),
        ("$10k", "diez mil dólares"),
        ("$1k", "mil dólares"),
        ("500 MXN", "quinientos pesos mexicanos"),
    ]);
}

#[test]
fn a_markdown_table_is_read_row_by_row() {
    let table = "| Plan | Precio | Notas |\n|:--|--:|:-:|\n| Básico | $5/mes | **ok** |\n| Pro | $12/mes | +18 % |";
    let spoken = normalize_for_speech(table);
    assert_eq!(
        spoken,
        "Plan, Precio, Notas.\n\
         Básico, cinco dólares al mes, ok.\n\
         Pro, doce dólares al mes, más dieciocho por ciento."
    );
    assert!(!spoken.contains('|') && !spoken.contains('-') && !spoken.contains("barra"));
}

#[test]
fn signs_ranges_and_symbols() {
    check(&[
        ("subió +18 %", "subió más dieciocho por ciento"),
        ("bajó −6 %", "bajó menos seis por ciento"),
        ("entre 10-20 dólares", "entre diez a veinte dólares"),
        ("de 2020–2024", "de dos mil veinte a dos mil veinticuatro"),
        ("Rinde ~20 %", "Rinde aproximadamente veinte por ciento"),
        ("nota #3", "nota número tres"),
        ("a × b", "a por b"),
        ("x ≥ y", "x mayor o igual que y"),
    ]);
}

#[test]
fn phones_dates_and_fractions() {
    check(&[
        (
            "Llama al 555-1234",
            "Llama al cinco cinco cinco, uno dos tres cuatro",
        ),
        (
            "55-1234-5678",
            "cinco cinco, uno dos tres cuatro, cinco seis siete ocho",
        ),
        ("2026-11-10", "diez de noviembre de dos mil veintiséis"),
        ("10/11/2026", "diez de noviembre de dos mil veintiséis"),
        ("abierto 24/7", "abierto todos los días, a todas horas"),
        ("1/2 y 3/4 y 5/8", "un medio y tres cuartos y cinco octavos"),
        ("7/9", "siete entre nueve"),
    ]);
}

#[test]
fn units_and_slashes_between_words() {
    check(&[
        ("120 km/h", "ciento veinte kilómetros por hora"),
        ("50 MB/s", "cincuenta megabytes por segundo"),
        ("5 tokens/s", "cinco tokens por segundo"),
        ("20 peticiones/h", "veinte peticiones por hora"),
        ("3/s", "tres por segundo"),
        ("entrada/salida", "entrada o salida"),
        ("Windows/Linux", "Windows o Linux"),
        ("y/o", "y o"),
    ]);
}

#[test]
fn no_slash_or_pipe_or_currency_symbol_survives_a_realistic_answer() {
    let answer = "Resumen de precios:\n\n| Plan | Mensual | Anual |\n|---|--:|--:|\n\
                  | Free | $0/mes | $0/año |\n| Pro | $20/mes | $200/año |\n\
                  | Equipo | $15/usuario/mes | $150/usuario/año |\n\n\
                  Pro sube +25 % respecto a 2025 (de $16 a $20) y el soporte es 24/7.";
    let spoken = normalize_for_speech(answer);
    for forbidden in ["/", "|", "$", "%", "+", "**", "  "] {
        assert!(!spoken.contains(forbidden), "{forbidden}: {spoken}");
    }
    for expected in [
        "veinte dólares al mes",
        "doscientos dólares al año",
        "quince dólares por usuario al mes",
    ] {
        assert!(spoken.contains(expected), "{expected}: {spoken}");
    }
}

#[test]
fn ordinary_sentences_pass_through_untouched_except_numbers() {
    for plain in [
        "Hola, ¿cómo estás?",
        "Son las tres y media.",
        "Mañana a las 10:30 nos vemos.",
        "No hay nada que normalizar aquí.",
        "El archivo se llama resumen final.",
    ] {
        let spoken = normalize_for_speech(plain);
        assert!(!spoken.contains('/') && !spoken.contains('|'), "{spoken}");
        assert!(
            spoken.starts_with(plain.split_whitespace().next().unwrap_or("")),
            "{spoken}"
        );
    }
}

#[test]
fn text_without_symbols_is_not_mangled() {
    assert_eq!(
        normalize_for_speech("Gracias por esperar, ya terminé."),
        "Gracias por esperar, ya terminé."
    );
    assert_eq!(normalize_for_speech(""), "");
    assert_eq!(normalize_for_speech("   "), "");
}
