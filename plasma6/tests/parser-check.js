#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const vm = require("vm");

function loadQmlLibrary(relativePath) {
  const filename = path.join(__dirname, relativePath);
  const source = fs.readFileSync(filename, "utf8").replace(/^\.pragma library\s*/m, "");
  const context = vm.createContext({ console });
  vm.runInContext(source, context, { filename });
  return context;
}

function fixture() {
  return JSON.parse(
    fs.readFileSync(
      path.join(__dirname, "../../api/test/fixtures/contract-menu.json"),
      "utf8"
    )
  );
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function checkNormalizedMenu(ApiAdapter) {
  const normalized = ApiAdapter.normalizePayload(
    fixture(),
    "tietoteknia",
    "2026-07-24",
    "en"
  );
  assert(!normalized.error, normalized.error);
  assert(normalized.restaurantName === "Tietoteknia", "restaurant name");
  assert(normalized.todayMenu.lunchTime === "10:30–14:00", "lunch hours");
  assert(normalized.todayMenu.menus.length === 4, "offers and groups");

  const buffet = normalized.todayMenu.menus[1];
  assert(buffet.audiencePrices, "audience price marker");
  assert(buffet.price.includes("Staff 13,30 €"), "staff price");
  assert(buffet.price.includes("Guest 13,30 €"), "guest price");
  assert(buffet.price.includes("Student 3,10 €"), "student price");
  assert(
    buffet.components[0] === "Härkäpapua tikka masala (G, L, M, Veg)",
    "item tags"
  );

  const untitled = normalized.todayMenu.menus[3];
  assert(untitled.name === "", "untitled group stays untitled");
  assert(untitled.price === "", "unpriced group stays unpriced");
  assert(untitled.components[0] === "Satokauden kasviksia", "unpriced item");
}

function checkGeneralOffers(ApiAdapter, MenuFormatter) {
  const payload = fixture();
  payload.restaurant.id = "pranzeria-sorrento";
  payload.restaurant.name = { fi: "Pranzeria Sorrento", en: "Pranzeria Sorrento" };
  payload.offers = [{
    id: "buffet",
    label: "Lounasbuffet",
    price: { amount: "14.00", currency: "EUR" },
    description: "Salaatti, antipasto, pizza ja pääruoka"
  }];
  payload.groups = [];

  const normalized = ApiAdapter.normalizePayload(
    payload,
    "pranzeria-sorrento",
    "2026-07-24",
    "fi"
  );
  const offer = normalized.todayMenu.menus[0];
  assert(offer.price === "14,00 €", "general offer price");
  assert(
    MenuFormatter.menuHeading(offer, false, true, true, true, false)
      === "Lounasbuffet",
    "master toggle hides only the price"
  );
  assert(
    offer.components[0] === "Salaatti, antipasto, pizza ja pääruoka",
    "general offer description"
  );
}

function checkAudienceFiltering(ApiAdapter, MenuFormatter) {
  const normalized = ApiAdapter.normalizePayload(
    fixture(),
    "tietoteknia",
    "2026-07-24",
    "en"
  );
  const buffet = normalized.todayMenu.menus[1];
  const staffOnly = MenuFormatter.menuHeading(
    buffet,
    true,
    false,
    true,
    false,
    false
  );
  assert(staffOnly.includes("Staff 13,30 €"), "staff price remains visible");
  assert(!staffOnly.includes("Student"), "student price is hidden");
  assert(!staffOnly.includes("Guest"), "guest price is hidden");

  const untitled = normalized.todayMenu.menus[3];
  assert(
    MenuFormatter.menuHeading(untitled, true, true, true, true, false) === "",
    "empty groups do not create a stray Menu heading"
  );
}

function checkServiceStates(ApiAdapter) {
  const closed = fixture();
  closed.service = { status: "closed" };
  closed.closure = {
    kind: "seasonal",
    startsOn: "2026-06-18",
    endsOn: "2026-08-09"
  };
  closed.offers = [];
  closed.groups = [];
  const normalized = ApiAdapter.normalizePayload(
    closed,
    "tietoteknia",
    "2026-07-24",
    "en"
  );
  assert(normalized.serviceState === "closed", "closed service state");
  assert(normalized.todayMenu === null, "closure omits redundant current date");
  assert(
    normalized.serviceMessage === "Closed until 9 August.",
    "closure end date"
  );

  const closedFinnish = ApiAdapter.normalizePayload(
    closed,
    "tietoteknia",
    "2026-07-24",
    "fi"
  );
  assert(
    closedFinnish.serviceMessage === "Suljettu 9. elokuuta asti.",
    "Finnish closure end date"
  );

  const unknown = fixture();
  unknown.service = { status: "unknown" };
  unknown.offers = [];
  unknown.groups = [];
  assert(
    ApiAdapter.normalizePayload(
      unknown,
      "tietoteknia",
      "2026-07-24",
      "fi"
    ).error === "Ruokalistaa ei saatavilla",
    "unknown status"
  );

  const future = fixture();
  future.service = { status: "temporarilyUnavailable" };
  future.offers = [];
  future.groups = [];
  assert(
    ApiAdapter.normalizePayload(
      future,
      "tietoteknia",
      "2026-07-24",
      "en"
    ).error === "Menu unavailable",
    "future status degrades to unknown"
  );
}

function checkHelsinkiDate(ApiAdapter) {
  assert(
    ApiAdapter.helsinkiDateIso(new Date("2026-01-01T22:30:00Z"))
      === "2026-01-02",
    "Helsinki winter date"
  );
  assert(
    ApiAdapter.helsinkiDateIso(new Date("2026-07-24T21:30:00Z"))
      === "2026-07-25",
    "Helsinki summer date"
  );
}

function checkRetryBudget(ApiAdapter) {
  const date = "2026-07-24";
  const now = 1000000;
  const first = ApiAdapter.retrySchedule(0, "", date, now);
  const second = ApiAdapter.retrySchedule(
    first.failureCount,
    first.retryDateIso,
    date,
    now
  );
  const third = ApiAdapter.retrySchedule(
    second.failureCount,
    second.retryDateIso,
    date,
    now
  );
  const fourth = ApiAdapter.retrySchedule(
    third.failureCount,
    third.retryDateIso,
    date,
    now
  );
  assert(first.nextRetryEpochMs === now + 5 * 60 * 1000, "first retry");
  assert(second.nextRetryEpochMs === now + 15 * 60 * 1000, "second retry");
  assert(third.nextRetryEpochMs === now + 60 * 60 * 1000, "third retry");
  assert(fourth.nextRetryEpochMs === 0, "daily retry cap");
  assert(
    ApiAdapter.retrySchedule(4, date, "2026-07-25", now).failureCount === 1,
    "retry budget resets on a new day"
  );
  assert(
    !ApiAdapter.automaticRetryDue(4, date, date, 0, now),
    "daily retry cap blocks implicit refreshes"
  );
  assert(
    ApiAdapter.automaticRefreshDue(true, 4, date, date, 0, now),
    "configured refresh bypasses the implicit daily cap"
  );
  assert(
    !ApiAdapter.automaticRefreshDue(false, 4, date, date, 0, now),
    "implicit refresh remains capped"
  );
  assert(
    !ApiAdapter.automaticRetryDue(
      first.failureCount,
      date,
      date,
      first.nextRetryEpochMs,
      now
    ),
    "retry delay blocks implicit refreshes"
  );
  assert(
    ApiAdapter.automaticRetryDue(
      first.failureCount,
      date,
      date,
      first.nextRetryEpochMs,
      first.nextRetryEpochMs
    ),
    "retry becomes due at its scheduled time"
  );
  assert(
    ApiAdapter.automaticRetryDue(4, date, "2026-07-25", 0, now),
    "implicit retry budget resets on a new day"
  );
}

function main() {
  const ApiAdapter = loadQmlLibrary("../contents/ui/ApiAdapter.js");
  const MenuFormatter = loadQmlLibrary("../contents/ui/MenuFormatter.js");
  checkNormalizedMenu(ApiAdapter);
  checkGeneralOffers(ApiAdapter, MenuFormatter);
  checkAudienceFiltering(ApiAdapter, MenuFormatter);
  checkServiceStates(ApiAdapter);
  checkHelsinkiDate(ApiAdapter);
  checkRetryBudget(ApiAdapter);
  process.stdout.write("Normalized API checks passed\n");
}

main();
