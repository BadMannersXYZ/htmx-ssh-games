use std::sync::LazyLock;

use anyhow::{anyhow, Context, Result};
use bitvec::{bitvec, order::Lsb0, vec::BitVec};
use regex::Regex;

use super::{populate_board, PopulatedBoard};

/// List of monochrome puzzles obtained from https://nonogrammed.com/
pub static NONOGRAMMED_PUZZLE_LIST: [u32; 593] = [
    // Small puzzles
    2663, 2659, 2658, 2657, 2656, 2655, 2654, 2653, 2652, 2651, 2643, 2634, 2621, 2619, 2617, 2615,
    2613, 2611, 2609, 2608, 2607, 2594, 2583, 2562, 2532, 2494, 2476, 2468, 2443, 2386, 2380, 2377,
    2337, 2322, 2321, 2308, 2275, 2257, 2252, 2251, 2221, 2215, 2207, 2179, 2167, 2116, 2099, 2095,
    2087, 2071, 2047, 2032, 1986, 1954, 1899, 1897, 1891, 1855, 1853, 1850, 1839, 1835, 1829, 1824,
    1815, 1811, 1810, 1809, 1801, 1798, 1796, 1794, 1790, 1788, 1787, 1782, 1752, 1726, 1720, 1651,
    1631, 1584, 1569, 1508, 1489, 1488, 1484, 1459, 1440, 1423, 1421, 1416, 1398, 1397, 1395, 1353,
    1337, 1327, 1248, 1101, 1098, 1085, 1055, 1028, 1025, 1021, 1011, 979, 978, 964, 957, 955, 954,
    949, 948, 947, 946, 941, 920, 914, 910, 882, 880, 837, 812, 809, 807, 806, 763, 665, 659, 658,
    648, 599, 557, 556, 555, 554, 553, 535, 529, 503, 489, 486, 442, 432, 424, 420, 407, 404, 335,
    281, 278, 267, 266, 265, 264, 263, 262, 253, 252, 249, 248, 165, 135, 133, 128, 101, 77, 76,
    64, 63, 59, 57, 56, 54, 53, 45, //
    // Medium puzzles
    2645, 2639, 2637, 2629, 2606, 2597, 2596, 2576, 2571, 2549, 2548, 2543, 2542, 2541, 2535, 2533,
    2529, 2526, 2525, 2524, 2515, 2508, 2505, 2487, 2485, 2482, 2481, 2480, 2478, 2475, 2467, 2457,
    2455, 2454, 2453, 2449, 2446, 2437, 2436, 2435, 2430, 2428, 2427, 2426, 2424, 2419, 2418, 2417,
    2407, 2399, 2398, 2397, 2396, 2391, 2385, 2379, 2372, 2366, 2356, 2353, 2350, 2347, 2341, 2336,
    2335, 2331, 2329, 2328, 2309, 2305, 2302, 2300, 2298, 2297, 2296, 2295, 2292, 2287, 2285, 2278,
    2276, 2269, 2267, 2264, 2244, 2236, 2226, 2213, 2212, 2191, 2183, 2181, 2180, 2178, 2177, 2168,
    2159, 2158, 2157, 2156, 2155, 2151, 2150, 2149, 2130, 2129, 2128, 2127, 2126, 2125, 2124, 2122,
    2110, 2107, 2106, 2100, 2094, 2093, 2090, 2070, 2068, 2051, 2045, 2044, 2031, 2018, 1968, 1967,
    1966, 1965, 1964, 1959, 1948, 1947, 1946, 1945, 1943, 1916, 1879, 1862, 1849, 1845, 1822, 1820,
    1819, 1812, 1795, 1775, 1773, 1772, 1763, 1762, 1760, 1758, 1740, 1738, 1735, 1730, 1706, 1704,
    1701, 1660, 1659, 1658, 1656, 1653, 1629, 1627, 1626, 1624, 1623, 1616, 1611, 1607, 1604, 1589,
    1562, 1548, 1537, 1534, 1510, 1509, 1502, 1501, 1498, 1497, 1496, 1495, 1494, 1493, 1492, 1491,
    1490, 1469, 1462, 1458, 1455, 1452, 1448, 1442, 1437, 1425, 1419, 1418, 1402, 1400, 1377, 1371,
    1369, 1364, 1348, 1338, 1336, 1300, 1287, 1286, 1284, 1245, 1242, 1239, 1237, 1236, 1234, 1212,
    1195, 1192, 1175, 1171, 1169, 1151, 1149, 1133, 1132, 1126, 1125, 1120, 1117, 1112, 1103, 1100,
    1096, 1090, 1080, 1079, 1071, 1068, 1066, 1062, 1059, 1053, 1052, 1050, 1048, 1046, 1039, 1037,
    1032, 1024, 1017, 1013, 1012, 1006, 998, 977, 975, 974, 973, 972, 960, 958, 952, 950, 942, 940,
    939, 921, 916, 913, 907, 904, 902, 898, 895, 893, 892, 868, 857, 848, 839, 818, 817, 810, 808,
    803, 801, 799, 794, 789, 764, 760, 738, 733, 732, 717, 715, 702, 701, 700, 699, 695, 685, 656,
    645, 643, 641, 635, 623, 621, 618, 617, 615, 606, 602, 595, 559, 558, 544, 543, 534, 533, 526,
    512, 502, 488, 485, 484, 470, 469, 425, 413, 412, 400, 322, 321, 318, 302, 293, 291, 290, 288,
    287, 285, 268, 236, 231, 203, 197, 196, 195, 194, 193, 192, 191, 190, 176, 161, 144, 143, 142,
    136, 129, 112, 111, 109, 102, 97, 94, 92, 91, 89, 88, 87, 86, 85, 81, 78, 74, 55, 52, 51, 49,
    48, 47, 46, 43, 34, 33, 32, 31, 30, 29, 26, 22, 21, 20, 19, 18, 11, 7, 6, 5, 4, 3, 2,
    1, //
];

#[derive(Clone)]
pub struct NonogrammedPuzzle {
    pub id: u32,
    pub title: Option<String>,
    pub copyright: Option<String>,
    pub rows: Vec<Vec<u8>>,
    pub columns: Vec<Vec<u8>>,
    pub solution: BitVec<usize, Lsb0>,
}

static USERNAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("<a href=['\"]user\\.php\\?NAME=(?P<username>[^'\"]+)['\"]>").unwrap()
});

static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "document\\.getElementById\\(['\"]title['\"]\\)\\.innerHTML\\s*=\\s*['\"]<[^>]+>(?P<title>[^<]+)<",
    )
    .unwrap()
});

static SOLUTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("var data\\s*=\\s*['\"](?P<solution>[01]+)['\"]").unwrap());

static ROWS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("var height\\s*=\\s*parseInt\\((?P<rows>\\d+)\\)").unwrap());

static COLUMNS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("var width\\s*=\\s*parseInt\\((?P<columns>\\d+)\\)").unwrap());

pub async fn get_puzzle_data(id: u32) -> Result<NonogrammedPuzzle> {
    let client = reqwest::Client::new();
    let html_response = client
        .get(format!("https://nonogrammed.com/index.php?NUM={id}"))
        .send()
        .await
        .with_context(|| "URL fetch error")?
        .text()
        .await
        .with_context(|| "Received non-text response")?;
    let mut title = None;
    let mut copyright = None;
    let mut rows = None;
    let mut columns = None;
    let mut solution = bitvec![];
    let mut lines = html_response.lines();
    loop {
        let Some(line) = lines.next() else {
            break;
        };
        if copyright.is_none() {
            if let Some(caps) = USERNAME_RE.captures(line) {
                let username = &caps["username"];
                copyright = Some(format!(
                    r#"&copy; Copyright <a href="https://nonogrammed.com/user.php?NAME={username}">{username}</a>"#
                ));
            }
        }
        if title.is_none() {
            if let Some(caps) = TITLE_RE.captures(line) {
                title = Some(String::from(&caps["title"]));
            }
        }
        if solution.is_empty() {
            if let Some(caps) = SOLUTION_RE.captures(line) {
                solution.extend(caps["solution"].chars().map(|char| char == '1'));
            }
        }
        if rows.is_none() {
            if let Some(caps) = ROWS_RE.captures(line) {
                rows = Some(
                    caps["rows"]
                        .parse::<u16>()
                        .with_context(|| "Invalid rows value.")?,
                );
            }
        }
        if columns.is_none() {
            if let Some(caps) = COLUMNS_RE.captures(line) {
                columns = Some(
                    caps["columns"]
                        .parse::<u16>()
                        .with_context(|| "Invalid columns value.")?,
                );
            }
        }
    }
    if solution.is_empty() {
        return Err(anyhow!("Missing solution."));
    }
    if rows.is_none() {
        return Err(anyhow!("Missing rows."));
    }
    if title.is_none() {
        return Err(anyhow!("Missing title."));
    }
    if copyright.is_none() {
        return Err(anyhow!("Missing copyright."));
    }
    if columns.is_none() {
        return Err(anyhow!("Missing columns."));
    }
    let PopulatedBoard { rows, columns } =
        populate_board(&solution, rows.unwrap(), columns.unwrap())?;
    Ok(NonogrammedPuzzle {
        id,
        title,
        copyright,
        rows,
        columns,
        solution,
    })
}
