/**
 * The Hardless Chrome extension. It injects `@hardless/shim-web` at `document_start` in the
 * page's MAIN world, only on origins the user has enabled (FR-17, NFR-03).
 *
 * Milestone M0 ships the manifest skeleton; injection arrives in milestone M3. Content
 * scripts are registered at runtime per enabled origin, so the static manifest lists none.
 *
 * @packageDocumentation
 */

/** Where and when the shim is injected (FR-17). */
export const INJECTION = { world: "MAIN", runAt: "document_start" } as const;
