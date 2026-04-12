# AI Desktop Notifications Implementation Plan

## Summary

This change adds desktop notifications for AI workspace states where the agent is waiting on the user:

- approvals
- structured user input requests
- plan-ready follow-up prompts
- ordinary turn completion when no higher-priority waiting state is present

The feature is config-backed, exposed in Settings, deduplicated across visible and background AI workspaces, and suppressed while Hunk is the active window. On macOS, notification permission is handled explicitly with `UNUserNotificationCenter`, with the first prompt triggered when the user enters the AI workspace while notifications are enabled.

## Engineering Decisions

- Notification scope is AI-only for v1.
- Delivery is additive to existing in-app prompts and panels.
- At most one desktop notification is emitted per AI snapshot update.
- Notification priority is:
  1. approval required
  2. user input required
  3. plan ready
  4. agent finished
- The first AI snapshot for a workspace seeds the notification baseline and never emits a notification.
- macOS permission prompting happens on first AI workspace activation, not on app launch.
- macOS delivery uses `UNUserNotificationCenter`.
- Linux and Windows delivery use `notify-rust`.
- Windows notifications set an explicit app ID.

## TODO

- [x] Add config types and defaults for desktop notifications.
- [x] Add Settings UI controls for AI desktop notifications.
- [x] Add platform notification backends for Linux, macOS, and Windows.
- [x] Add macOS permission status and request handling.
- [x] Add AI attention-state diffing and deduplication.
- [x] Trigger permission checks when entering AI workspace.
- [x] Trigger desktop notifications from both visible and background AI snapshot paths.
- [x] Add tests for config defaults and AI notification decision logic.
- [x] Run workspace build, clippy, and tests after implementation.

## Implementation Notes

- Config lives in `hunk-domain` so desktop and future frontends can share the same schema.
- Notification decision logic should stay pure and testable, independent from GPUI and OS backends.
- The visible AI runtime and hidden workspace runtime must use the same attention diff code.
- The notification baseline should advance even when notifications are suppressed because the Hunk window is focused.
- macOS permission state should be cached in app state and refreshed after explicit authorization requests.
- Denied macOS permission should surface as a small explanatory note in Settings instead of an error toast.
