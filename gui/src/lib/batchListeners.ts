/** Keep a batch subscription only when every listener registered and the view is live. */
export function collectBatchListeners(
  registrations: PromiseSettledResult<() => void>[],
  live: boolean,
): Array<() => void> | null {
  const registered = registrations.flatMap((registration) =>
    registration.status === "fulfilled" ? [registration.value] : [],
  );
  const failed = registrations.find(
    (registration): registration is PromiseRejectedResult =>
      registration.status === "rejected",
  );
  if (failed || !live) {
    registered.forEach((unlisten) => unlisten());
    if (failed) throw failed.reason;
    return null;
  }
  return registered;
}
