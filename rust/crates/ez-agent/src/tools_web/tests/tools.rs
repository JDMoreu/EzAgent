//! Pruebas de las herramientas web: construcción, búsqueda, validación y transporte fijado (separadas por tamaño).

use super::*;

#[test]
fn builds_the_two_level_zero_tools() {
    let (_dir, config) = config(false, 200);
    let provider: Arc<dyn SearchProvider> = Arc::new(FakeProvider::new(Vec::new()));
    let tools = build_web_tools(&config, Some(provider));
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "web_search");
    assert_eq!(tools[1].name, "fetch_page");
    for tool in &tools {
        assert_eq!(tool.level, 0);
        assert!(tool.untrusted_output);
        assert_eq!(tool.timeout_s, 10.0);
    }
    assert_eq!(
        tools[0].parameters["properties"]["max_results"]["maximum"],
        json!(10)
    );
    assert_eq!(
        tools[1].parameters["properties"]["max_chars"]["maximum"],
        json!(200)
    );
    assert_eq!(
        tools[1].parameters["properties"]["max_chars"]["default"],
        json!(200)
    );
    let query = json!({"query": "  hola  "});
    assert_eq!(
        tools[0]
            .describe(query.as_object().expect("objeto"))
            .summary,
        "Buscar en la web «hola»"
    );
    let url = json!({"url": "https://a.test"});
    assert_eq!(
        tools[1].describe(url.as_object().expect("objeto")).summary,
        "Leer la página https://a.test"
    );
    let registry = crate::registry::ToolRegistry::new();
    registry.register_many(tools);
    assert!(registry.validate("fetch_page", &json!({})).is_some());
    assert!(registry.validate("web_search", &json!({})).is_some());
}

#[test]
fn wraps_search_results_and_caps_them() {
    let results = vec![
        SearchResult {
            title: "Uno".into(),
            url: "https://uno.test".into(),
            snippet: "primero".into(),
        },
        SearchResult {
            title: String::new(),
            url: "https://dos.test".into(),
            snippet: "segundo".into(),
        },
    ];
    let provider = FakeProvider::new(results);
    let result = run_web_search(&provider, "  clima  ", 5);
    assert_eq!(provider.calls.load(Ordering::Relaxed), 1);
    assert!(result.ok);
    let data = result.data.as_str().unwrap_or_default();
    assert!(data.starts_with("<untrusted_content source='web_search'>"));
    assert!(data.contains("1. Uno\nURL: https://uno.test\nFragmento: primero"));
    assert!(data.contains("2. (sin título)\nURL: https://dos.test\nFragmento: segundo"));
    let many: Vec<SearchResult> = (0..10)
        .map(|index| SearchResult {
            title: format!("t{index}"),
            url: format!("u{index}"),
            snippet: String::new(),
        })
        .collect();
    let provider = FakeProvider::new(many);
    let result = run_web_search(&provider, "x", 3);
    assert_eq!(
        result
            .data
            .as_str()
            .unwrap_or_default()
            .matches("URL:")
            .count(),
        3
    );
}

#[test]
fn empty_search_is_not_a_failure() {
    let provider = FakeProvider::new(Vec::new());
    let result = run_web_search(&provider, "nada", 5);
    assert!(result.ok);
    assert_eq!(result.error_kind.as_deref(), Some("empty"));
    assert!(result
        .data
        .as_str()
        .unwrap_or_default()
        .contains("Sin resultados para «nada»."));
}

#[test]
fn search_arguments_are_validated_before_calling_the_provider() {
    let provider = FakeProvider::new(Vec::new());
    let long = "x".repeat(MAX_QUERY_CHARS + 1);
    let cases = [
        (json!(""), json!(5), "vacía"),
        (json!("  "), json!(5), "vacía"),
        (json!(null), json!(5), "cadena"),
        (json!(long), json!(5), "larga"),
        (json!("ok"), json!(0), "entre 1 y 10"),
        (json!("ok"), json!(11), "entre 1 y 10"),
        (json!("ok"), json!("5"), "entre 1 y 10"),
        (json!("ok"), json!(true), "entre 1 y 10"),
    ];
    for (query, limit, expected) in cases {
        let result = run_web_search_values(&provider, Some(&query), Some(&limit));
        assert!(
            result.error.unwrap_or_default().contains(expected),
            "{query:?}/{limit:?}"
        );
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn provider_failures_keep_their_kind() {
    for (error, kind) in [
        (SearchError::unavailable("sin proveedor"), "unavailable"),
        (SearchError::new("boom"), "provider"),
        (SearchError::with_kind("lento", "timeout"), "timeout"),
    ] {
        let provider = FakeProvider::failing(error.clone());
        let result = run_web_search(&provider, "x", 5);
        assert!(!result.ok);
        assert_eq!(result.error.as_deref(), Some(error.message.as_str()));
        assert_eq!(result.error_kind.as_deref(), Some(kind));
    }
}

#[test]
fn fetch_arguments_are_validated_in_python_order() {
    let (_dir, config) = config(false, 200);
    let bad_url = json!(5);
    let result = run_fetch_page_values(
        &config,
        Some(&bad_url),
        Some(&json!(0)),
        &Default::default(),
    );
    assert_eq!(
        result.error.as_deref(),
        Some("El parámetro 'max_chars' debe ser mayor que cero.")
    );
    let result = run_fetch_page_values(
        &config,
        Some(&bad_url),
        Some(&json!(10)),
        &Default::default(),
    );
    assert_eq!(
        result.error.as_deref(),
        Some("La URL debe ser una cadena de texto.")
    );
    let result = run_fetch_page_values(&config, None, Some(&json!(10)), &Default::default());
    assert_eq!(
        result.error.as_deref(),
        Some("La URL no puede estar vacía.")
    );
}

struct FixedResolver(Vec<std::net::IpAddr>);

impl HostResolver for FixedResolver {
    fn resolve(&self, _host: &str, _port: u16) -> Option<Vec<std::net::IpAddr>> {
        Some(self.0.clone())
    }
}

#[test]
fn pinned_target_keeps_the_host_header_and_sni() {
    let (_dir, config) = config(true, 8000);
    let public: std::net::IpAddr = "93.184.216.34".parse().expect("IP pública");
    let resolver = FixedResolver(vec![public]);
    let (connect, headers, extensions) =
        pinned_request_target_with("https://example.test:8443/a?b=1#frag", &config, &resolver)
            .expect("destino fijado");
    assert_eq!(connect, "https://93.184.216.34:8443/a?b=1");
    assert_eq!(
        headers,
        vec![("Host".to_string(), "example.test:8443".to_string())]
    );
    assert_eq!(
        extensions,
        vec![("sni_hostname".to_string(), "example.test".to_string())]
    );

    let (connect, headers, extensions) =
        pinned_request_target_with("https://example.test/a", &config, &resolver).expect("destino");
    assert_eq!(connect, "https://93.184.216.34/a");
    assert_eq!(
        headers,
        vec![("Host".to_string(), "example.test".to_string())]
    );
    assert_eq!(
        extensions,
        vec![("sni_hostname".to_string(), "example.test".to_string())]
    );

    let (connect, headers, extensions) =
        pinned_request_target_with("http://example.test/a", &config, &resolver).expect("destino");
    assert_eq!(connect, "http://93.184.216.34/a");
    assert_eq!(
        headers,
        vec![("Host".to_string(), "example.test".to_string())]
    );
    assert!(extensions.is_empty());

    let (_connect, headers, extensions) =
        pinned_request_target_with("https://españa.test/a", &config, &resolver).expect("destino");
    assert_eq!(
        headers,
        vec![("Host".to_string(), "xn--espaa-rta.test".to_string())]
    );
    assert_eq!(
        extensions,
        vec![("sni_hostname".to_string(), "xn--espaa-rta.test".to_string())]
    );
}

#[test]
fn pinned_target_blocks_a_private_resolution() {
    let (_dir, config) = config(true, 8000);
    let resolver = FixedResolver(vec!["10.0.0.5".parse().expect("IP privada")]);
    let error = pinned_request_target_with("https://interno.test/", &config, &resolver)
        .expect_err("la IP privada no se permite");
    assert_eq!(error.kind, WebToolErrorKind::UnsafeUrl);
}

struct CapturingTransport {
    seen: std::sync::Mutex<Vec<WebRequest>>,
}

impl WebTransport for CapturingTransport {
    fn request(&self, request: &WebRequest) -> Result<WebResponse, TransportError> {
        self.seen.lock().expect("peticiones").push(request.clone());
        Ok(WebResponse {
            status: 200,
            headers: vec![("content-type".to_string(), "text/html".to_string())],
            body: b"<p>hola</p>".to_vec(),
        })
    }
}

#[test]
fn injected_transport_sees_the_original_url() {
    let (_dir, config) = config(true, 8000);
    let transport = Arc::new(CapturingTransport {
        seen: std::sync::Mutex::new(Vec::new()),
    });
    let public: std::net::IpAddr = "93.184.216.34".parse().expect("IP pública");
    let hooks = FetchHooks {
        transport: Some(transport.clone() as Arc<dyn WebTransport>),
        resolver: Some(Arc::new(FixedResolver(vec![public]))),
        ..FetchHooks::default()
    };
    let result = run_fetch_page_json(
        &config,
        &json!("http://example.test/p"),
        &json!(8000),
        &hooks,
    );
    assert!(result.ok, "{result:?}");
    let seen = transport.seen.lock().expect("peticiones");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].url, "http://example.test/p");
}
