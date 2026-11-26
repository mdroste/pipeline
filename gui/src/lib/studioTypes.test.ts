import { expectTypeOf, it } from "vitest";
import { studioClient } from "./studioClient";
import type { ProjectRecord } from "./projectTypes";
import type {
  BuildReceipt,
  BuildRecord,
  ResponseRecord,
  SavedText,
  StudioAction,
} from "./studioTypes";

// The function is typechecked, never invoked: these assertions exercise inference
// at real call sites without sending commands or fabricating native responses.
function contracts(
  save: Extract<StudioAction, { action: "saveText" }>,
  response: Extract<StudioAction, { action: "saveResponse" }>,
) {
  expectTypeOf(studioClient.mutate("project", save)).toEqualTypeOf<
    Promise<SavedText>
  >();
  expectTypeOf(studioClient.mutate("project", response)).toEqualTypeOf<
    Promise<ProjectRecord<ResponseRecord>>
  >();
  expectTypeOf(studioClient.records("project", "build")).toEqualTypeOf<
    Promise<ProjectRecord<BuildRecord | BuildReceipt>[]>
  >();
  // @ts-expect-error A caller cannot claim a result unrelated to its action.
  studioClient.mutate<ResponseRecord>("project", save);
  // @ts-expect-error The record kind selects its body; it is not a caller assertion.
  studioClient.records<ResponseRecord>("project", "build");
}

it("checks action and record inference at compile time", () => {
  expectTypeOf(contracts).toBeFunction();
});
