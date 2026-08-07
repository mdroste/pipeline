describe("packaged desktop startup", () => {
  it("renders the main window and opens Settings", async () => {
    await expect(browser).toHaveTitle("Pipeline");

    const app = await $('[data-testid="app-shell"]');
    await app.waitForDisplayed({ timeout: 30_000 });

    const dependencyModal = await $('[data-testid="dependencies-modal"]');
    if (await dependencyModal.isExisting()) {
      await $('[data-testid="dependencies-dismiss"]').click();
      await dependencyModal.waitForExist({ reverse: true });
    }

    await $("button=Settings").click();
    await $('[data-testid="settings-page"]').waitForDisplayed({ timeout: 30_000 });
    await expect($("h2=Settings")).toBeDisplayed();
  });
});
