// Moves a node to <body> so popups escape clipping and transformed
// ancestors. Svelte 5 listens on the document too, so handlers keep working.
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
