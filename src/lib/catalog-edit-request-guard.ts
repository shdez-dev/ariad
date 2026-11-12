export function createCatalogEditRequestGuard() {
  let requestNumber = 0;

  return {
    current(): number {
      return requestNumber;
    },
    isCurrent(request: number): boolean {
      return request === requestNumber;
    },
    async run<T>(
      load: () => Promise<T>,
      apply: (value: T) => void,
      reportError: (error: unknown) => void,
    ): Promise<"applied" | "failed" | "superseded"> {
      const currentRequest = ++requestNumber;
      let value: T;
      try {
        value = await load();
      } catch (error: unknown) {
        if (currentRequest !== requestNumber) return "superseded";
        reportError(error);
        return "failed";
      }
      if (currentRequest !== requestNumber) return "superseded";
      apply(value);
      return "applied";
    },
    invalidate(): number {
      return ++requestNumber;
    },
  };
}
