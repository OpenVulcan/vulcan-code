import { expect, it } from "vitest";
import { accountInitial } from "./account-avatar";

// The initial is one whole user-perceived character, upper-cased where the script has case.
// 首字符是一个完整的用户可感知字符；有大小写的文字转为大写。
it("returns the first whole character of the account name", () => {
  expect(accountInitial("  lingeasy")).toBe("L");
  expect(accountInitial("λambda")).toBe("Λ");
  expect(accountInitial("👩‍💻 dev")).toBe("👩‍💻");
  expect(accountInitial("   ")).toBeNull();
  expect(accountInitial(null)).toBeNull();
});
