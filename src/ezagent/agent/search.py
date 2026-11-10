"""Proveedores de búsqueda web intercambiables.

Este módulo no importa dependencias opcionales al cargarse. El proveedor por
defecto usa el paquete ``ddgs`` (DuckDuckGo) con importación perezosa; para
añadir Brave Search o SearXNG en el futuro basta con implementar el protocolo
:class:`SearchProvider` y pasar la instancia a ``build_web_tools``.
"""

from __future__ import annotations

from typing import Any, Protocol, TypedDict, runtime_checkable


class SearchResult(TypedDict):
    """Resultado normalizado de una búsqueda."""

    title: str
    url: str
    snippet: str


class SearchError(RuntimeError):
    """Error de búsqueda con un mensaje en español apto para el usuario."""


class SearchUnavailableError(SearchError):
    """El proveedor no está disponible (por ejemplo, falta su paquete)."""


@runtime_checkable
class SearchProvider(Protocol):
    """Contrato mínimo de un proveedor de búsqueda.

    Implementaciones futuras (Brave, SearXNG) solo tienen que devolver una
    lista de :class:`SearchResult` y lanzar :class:`SearchError` con un
    mensaje en español cuando algo falle.
    """

    def search(self, query: str, max_results: int = 5) -> list[SearchResult]:
        """Busca ``query`` y devuelve como mucho ``max_results`` resultados."""
        ...


class DdgsSearchProvider:
    """Proveedor de búsqueda basado en el paquete opcional ``ddgs``.

    ``ddgs`` se importa dentro de ``search`` para que el núcleo arranque aunque
    la dependencia no esté instalada. Si falta, se lanza
    :class:`SearchUnavailableError` con instrucciones de instalación.
    """

    def __init__(
        self,
        *,
        region: str = "wt-wt",
        backend: str | None = None,
        timeout_s: float = 8.0,
    ) -> None:
        self._region = region
        self._backend = backend
        self._timeout_s = timeout_s

    def search(self, query: str, max_results: int = 5) -> list[SearchResult]:
        if not isinstance(query, str):
            raise SearchError("La consulta de búsqueda debe ser una cadena de texto.")
        normalized_query = query.strip()
        if not normalized_query:
            raise SearchError("La consulta de búsqueda no puede estar vacía.")
        if (
            isinstance(max_results, bool)
            or not isinstance(max_results, int)
            or not 1 <= max_results <= 10
        ):
            raise SearchError("El número de resultados debe estar entre 1 y 10.")

        ddgs_class = self._load_ddgs()
        try:
            engine = self._new_engine(ddgs_class)
            raw_results = self._run(engine, normalized_query, max_results)
        except SearchError:
            raise
        except Exception as exc:  # noqa: BLE001 - se traduce a un error de usuario
            # DECISION: ddgs lanza DDGSException("No results found.") cuando no
            # hay coincidencias; eso no es un fallo, es una lista vacía.
            if "no results found" in str(exc).lower():
                return []
            raise SearchError(f"La búsqueda web falló: {exc}") from exc

        return self._normalize(raw_results, max_results)

    @staticmethod
    def _load_ddgs() -> Any:
        """Carga ``ddgs.DDGS`` de forma perezosa."""
        try:
            from ddgs import DDGS  # type: ignore[import-not-found]
        except ImportError as exc:
            missing = getattr(exc, "name", None) or ""
            if missing == "" or missing == "ddgs" or missing.startswith("ddgs."):
                raise SearchUnavailableError(
                    "La búsqueda web no está disponible porque falta el paquete "
                    "opcional «ddgs». Instálalo con: pip install ddgs"
                ) from exc
            raise SearchUnavailableError(
                f"No se pudo cargar el proveedor de búsqueda «ddgs»: {exc}"
            ) from exc
        return DDGS

    def _new_engine(self, ddgs_class: Any) -> Any:
        try:
            return ddgs_class(timeout=self._timeout_s)
        except TypeError:
            # DECISION: algunas versiones de ddgs no aceptan timeout en el
            # constructor; se usa el valor por defecto del paquete.
            return ddgs_class()

    def _run(self, engine: Any, query: str, max_results: int) -> Any:
        kwargs: dict[str, Any] = {
            "region": self._region,
            "max_results": max_results,
        }
        if self._backend:
            kwargs["backend"] = self._backend
        return engine.text(query, **kwargs)

    @staticmethod
    def _normalize(raw_results: Any, max_results: int) -> list[SearchResult]:
        """Acepta ``href``/``url`` y ``body``/``snippet`` según la versión."""
        results: list[SearchResult] = []
        for item in raw_results or []:
            if not isinstance(item, dict):
                continue
            title = str(item.get("title") or "").strip()
            url = str(item.get("href") or item.get("url") or "").strip()
            snippet = str(
                item.get("body") or item.get("snippet") or item.get("description") or ""
            ).strip()
            if not title and not url:
                continue
            results.append({"title": title, "url": url, "snippet": snippet})
            if len(results) >= max_results:
                break
        return results
