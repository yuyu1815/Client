export function createLanguageRequests<T>(
  update: (language: string) => Promise<T>,
): {
  readonly generation: number;
  isCurrent(generation: number): boolean;
  set(language: string): Promise<T>;
};
