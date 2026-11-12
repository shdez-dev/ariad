export type UnitsRefreshRequest = {
  refreshNumber: number;
  detailRequestNumber: number;
};

export function createUnitsViewRequestGuard() {
  let refreshNumber = 0;
  let detailRequestNumber = 0;

  function isLatestRefresh(request: UnitsRefreshRequest): boolean {
    return request.refreshNumber === refreshNumber;
  }

  function canApplyRefreshResult(request: UnitsRefreshRequest): boolean {
    return isLatestRefresh(request) && request.detailRequestNumber === detailRequestNumber;
  }

  return {
    beginRefresh(): UnitsRefreshRequest {
      detailRequestNumber += 1;
      return {
        refreshNumber: ++refreshNumber,
        detailRequestNumber,
      };
    },
    beginDetailRequest(): number {
      return ++detailRequestNumber;
    },
    isLatestRefresh,
    canApplyRefreshResult,
    isCurrentDetailRequest(requestNumber: number): boolean {
      return requestNumber === detailRequestNumber;
    },
  };
}
