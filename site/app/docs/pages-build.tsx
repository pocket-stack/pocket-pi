import type { DocRecord } from "./doc-components";
import { BUILD_ADVANCED_DOCS } from "./pages-build-advanced";
import { BUILD_CORE_DOCS } from "./pages-build-core";

export const BUILD_DOCS: DocRecord[] = [
  ...BUILD_CORE_DOCS,
  ...BUILD_ADVANCED_DOCS,
];
