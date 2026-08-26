function setValue() {
  return PocketPi.data.transaction(() => {
    PocketPi.data.exec("UPDATE state SET value = 'clicked'");
    return { value: "clicked" };
  });
}

PocketPi.defineActions({ setValue });
