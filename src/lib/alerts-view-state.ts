import type { AlertDto } from "./ipc";
import { createAlertsRefreshGuard, type AlertsRefreshToken } from "./alerts-refresh-guard.ts";

export type AlertsViewSnapshot = {
  alerts: AlertDto[];
  loadFailed: boolean;
};

export function createAlertsViewState() {
  let alerts: AlertDto[] = [];
  let loadFailed = false;
  const refreshGuard = createAlertsRefreshGuard();

  return {
    beginRefresh(): AlertsRefreshToken {
      return refreshGuard.begin();
    },
    applyRefreshSuccess(request: AlertsRefreshToken, nextAlerts: AlertDto[]): boolean {
      if (!refreshGuard.canApply(request)) return false;
      alerts = nextAlerts;
      loadFailed = false;
      return true;
    },
    applyRefreshFailure(request: AlertsRefreshToken): boolean {
      if (!refreshGuard.canApply(request)) return false;
      loadFailed = true;
      return true;
    },
    applyReviewed(updated: AlertDto): void {
      refreshGuard.invalidate();
      alerts = alerts.map((alert) => alert.id === updated.id ? updated : alert);
    },
    isLatest(request: AlertsRefreshToken): boolean {
      return refreshGuard.isLatest(request);
    },
    snapshot(): AlertsViewSnapshot {
      return { alerts: [...alerts], loadFailed };
    },
  };
}
