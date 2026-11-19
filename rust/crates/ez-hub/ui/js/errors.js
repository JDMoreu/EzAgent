// Captura de errores de JS desde el primer momento, antes de que cargue el módulo.
// La usa la autocomprobación de la vista (brief ECO-5b).
window.__ezErrors = [];
window.onerror = (message) => {
  window.__ezErrors.push(String(message));
};
window.addEventListener("unhandledrejection", (event) => {
  window.__ezErrors.push(String(event.reason));
});
