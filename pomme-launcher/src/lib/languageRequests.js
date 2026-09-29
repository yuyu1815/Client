/** Serialize writes so the backend's persisted value and UI response order stay aligned.
 * @template T
 * @param {(language: string) => Promise<T>} update
 * @returns {{ generation: number, set: (language: string) => Promise<T> }}
 */
export function createLanguageRequests(update) {
  let pending = Promise.resolve();
  let generation = 0;

  return {
    get generation() {
      return generation;
    },
    isCurrent(requestGeneration) {
      return generation === requestGeneration;
    },
    set(language) {
      generation += 1;
      const request = pending.then(
        () => update(language),
        () => update(language),
      );
      pending = request.then(
        () => undefined,
        () => undefined,
      );
      return request;
    },
  };
}
