/** 検索欄の待ち時間（src/queryDelay.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";
import { endsWithPartialYear, queryDelayMs, QUERY_DELAY_MS, QUERY_DELAY_PARTIAL_YEAR_MS } from "../src/queryDelay.ts";

test("打ちかけの year: は長く待つ（0件の案内を一瞬出さない）", () => {
  for (const s of ["year:", "year:2", "year:20", "year:202", "YEAR:202", "年:202", "年:２０２", "沖縄 year:20"]) {
    assert.equal(endsWithPartialYear(s), true, s);
    assert.equal(queryDelayMs(s), QUERY_DELAY_PARTIAL_YEAR_MS, s);
  }
});

test("4桁そろった年・ほかの語・途中にある year: はいつもの待ち", () => {
  for (const s of ["year:2023", "年:２０２３", "year:2023 沖縄", "year:20 沖縄", "沖縄", "", "fyear:20", "camera:20"]) {
    assert.equal(endsWithPartialYear(s), false, s);
    assert.equal(queryDelayMs(s), QUERY_DELAY_MS, s);
  }
});
