/** The dashboard shell. Device, printer, drawer and fault panels arrive in milestone M1. */
export function App() {
  return (
    <main className="min-h-screen bg-white p-6 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <h1 className="text-2xl font-semibold">Hardless</h1>
      <p className="mt-2 text-neutral-600 dark:text-neutral-400">
        No devices yet. Start the server with <code>hardless start</code>.
      </p>
    </main>
  );
}
