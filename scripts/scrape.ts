import "../src/Utils.ts";

import {scrapeFoliaVersions} from "./scrape/scrape_folia_versions.ts";
import {scrapeVanillaVersions} from "./scrape/scrape_vanilla_versions.ts";
import {scrapePaperVersions} from "./scrape/scrape_paper_versions.ts";
import {scrapePurpurVersions} from "./scrape/scrape_purpur_versions.ts";
import {scrapeSpigotVersions} from "./scrape/scrape_spigot_versions.ts";

await scrapeVanillaVersions();
await scrapePaperVersions();
await scrapeFoliaVersions();
await scrapePurpurVersions();
await scrapeSpigotVersions();
