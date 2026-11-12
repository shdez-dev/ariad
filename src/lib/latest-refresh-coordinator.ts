export type LatestRefreshResult =
  | { kind: "loaded" }
  | { kind: "failed"; message: string }
  | { kind: "superseded" };

type LatestRefreshHandlers<T> = {
  onLoaded: (value: T) => void;
  onError: (error: unknown) => string;
  onSettled: () => void;
};

export function createLatestRefreshCoordinator() {
  let latestRequest = 0;

  return {
    async run<T>(
      load: () => Promise<T>,
      handlers: LatestRefreshHandlers<T>,
    ): Promise<LatestRefreshResult> {
      const request = ++latestRequest;

      try {
        const value = await load();
        if (request !== latestRequest) return { kind: "superseded" };
        handlers.onLoaded(value);
        return { kind: "loaded" };
      } catch (error: unknown) {
        if (request !== latestRequest) return { kind: "superseded" };
        return { kind: "failed", message: handlers.onError(error) };
      } finally {
        if (request === latestRequest) handlers.onSettled();
      }
    },
  };
}
