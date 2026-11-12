export interface AlertsRefreshToken {
  requestId: number;
  dataRevision: number;
}

export function createAlertsRefreshGuard() {
  let latestRequestId = 0;
  let dataRevision = 0;

  return {
    begin(): AlertsRefreshToken {
      latestRequestId += 1;
      return { requestId: latestRequestId, dataRevision };
    },
    invalidate(): void {
      dataRevision += 1;
    },
    isLatest(token: AlertsRefreshToken): boolean {
      return token.requestId === latestRequestId;
    },
    canApply(token: AlertsRefreshToken): boolean {
      return token.requestId === latestRequestId && token.dataRevision === dataRevision;
    },
  };
}
