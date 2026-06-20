import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

const isDesktopRuntime = (): boolean =>
  typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

export const ensureNotificationPermission = async (): Promise<boolean> => {
  if (!isDesktopRuntime()) {
    return false;
  }

  if (await isPermissionGranted()) {
    return true;
  }

  const permission = await requestPermission();
  return permission === "granted";
};

export const sendChannelUpdateNotification = (
  title: string,
  body: string,
): boolean => {
  if (!isDesktopRuntime()) {
    return false;
  }

  sendNotification({
    title,
    body,
  });
  return true;
};
