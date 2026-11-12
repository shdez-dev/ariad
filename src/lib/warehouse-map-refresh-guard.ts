export function createWarehouseMapRefreshGuard() {
  let latestRefresh = 0;
  let latestDetailRequest = 0;

  return {
    beginRefresh(): number {
      latestRefresh += 1;
      latestDetailRequest += 1;
      return latestRefresh;
    },
    beginDetailRequest(): number {
      latestDetailRequest += 1;
      return latestDetailRequest;
    },
    isLatestRefresh(request: number): boolean {
      return request === latestRefresh;
    },
    isCurrentDetailRequest(request: number): boolean {
      return request === latestDetailRequest;
    },
  };
}
