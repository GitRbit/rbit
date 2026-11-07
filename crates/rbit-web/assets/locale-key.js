// Versioned, bounded catalog identity. This is presentation state, never authority.
export function catalogKey(value) {
  return typeof value === 'string' && /^[1-9][0-9]{0,5}\.(en|es|en-XA|ar-XB)$/.test(value) ? value : null;
}

export function documentCatalogKey(document) {
  return catalogKey(document.documentElement?.getAttribute?.('data-catalog-key'));
}
