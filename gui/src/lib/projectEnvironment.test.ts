import { beforeEach, expect, it } from "vitest";
import { defaultProjectEnvironment } from "./projectEnvironment";
import { router } from "./router";

beforeEach(() => {
  localStorage.clear();
  router.init({ page: "home" });
  router.setGuard(null);
});

it("navigates with the real router when the method is called detached", async () => {
  // ProjectOverview destructures `navigate` and calls it without a receiver.
  const { navigate } = defaultProjectEnvironment;
  await expect(navigate({ page: "history", runId: "run-1" })).resolves.toBe(
    true,
  );
  expect(router.route).toMatchObject({ page: "history", runId: "run-1" });
});

it("still runs the navigation guard", async () => {
  router.setGuard(async () => false);
  const { navigate } = defaultProjectEnvironment;
  await expect(navigate({ page: "tasks", taskId: "task-1" })).resolves.toBe(
    false,
  );
  expect(router.route).toMatchObject({ page: "home" });
});
