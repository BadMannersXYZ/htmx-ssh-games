use std::sync::LazyLock;

use anyhow::{anyhow, Context, Result};
use bitvec::{bitvec, order::Lsb0, vec::BitVec};
use rand::seq::SliceRandom;
use rand::thread_rng;
use reqwest::redirect::Policy;

/// List of Nonogram puzzles obtained from https://webpbn.com/find.cgi with parameters:
///
/// > `search=1&status=0&minid=&maxid=&title=&author=&minsize=0&maxsize=400&minqual=4&maxqual=20&unqual=1&mindiff=4&maxdiff=15&undiff=1&mincolor=2&maxcolor=2&uniq=1&guess=3&blots=2&showcreate=1&order=0&perpage=0&save_settings=on`
pub const WEBPBN_PUZZLE_LIST: LazyLock<[u32; 871]> = LazyLock::new(|| {
    let mut list = [
        23, 141, 252, 439, 748, 831, 1340, 1445, 1568, 1809, 1871, 1915, 2123, 2413, 2676, 3321,
        3339, 3375, 3791, 3994, 4005, 4015, 4051, 4374, 4492, 4494, 4533, 4573, 4610, 4699, 4705,
        4755, 5113, 5194, 5269, 5527, 5703, 5733, 5737, 5745, 5775, 5858, 5860, 5863, 5878, 5906,
        5919, 6022, 6120, 6162, 6182, 6195, 6200, 6275, 6300, 6302, 6324, 6336, 6394, 6425, 6430,
        6449, 6493, 6507, 6510, 6520, 6539, 6542, 6583, 6595, 6610, 6611, 6618, 6619, 6622, 6627,
        6633, 6637, 6640, 6645, 6648, 6659, 6664, 6670, 6673, 6688, 6695, 6696, 6763, 6769, 6772,
        6777, 6781, 6790, 6791, 6795, 6796, 6799, 6822, 6829, 6834, 6842, 6845, 6855, 6925, 6947,
        6953, 6965, 6986, 7033, 7134, 7163, 7199, 7200, 7282, 7296, 7306, 7405, 7432, 7550, 7694,
        7707, 7714, 7943, 7961, 7996, 8076, 8098, 8105, 8113, 8137, 8149, 8155, 8177, 8222, 8225,
        8232, 8256, 8275, 8283, 8302, 8339, 8357, 8363, 8381, 8389, 8396, 8463, 8565, 8686, 8764,
        8765, 8769, 8902, 8918, 8922, 9038, 9063, 9101, 9152, 9182, 9184, 9216, 9240, 9259, 9313,
        9398, 9409, 9417, 9450, 9542, 9720, 9727, 10004, 10043, 10045, 10121, 10152, 10289, 10365,
        10378, 10381, 10391, 10415, 10482, 10500, 10596, 10613, 10640, 10665, 10687, 10724, 10739,
        10854, 10873, 10979, 11007, 11120, 11145, 11192, 11194, 11309, 11392, 11399, 11419, 11713,
        11719, 11880, 11948, 11963, 11987, 12034, 12138, 12176, 12341, 12349, 12354, 12356, 12434,
        12466, 12620, 12692, 12917, 13181, 13187, 13362, 13486, 13497, 13510, 13522, 13593, 13716,
        13830, 13832, 13861, 14009, 14080, 14081, 14102, 14104, 14109, 14118, 14127, 14142, 14255,
        14274, 14279, 14280, 14287, 14300, 14302, 14351, 14361, 14375, 14396, 14551, 14660, 14957,
        15253, 15263, 15271, 15306, 15322, 15325, 15389, 15398, 15403, 15435, 15451, 15506, 15735,
        15816, 15855, 15883, 15890, 15910, 15912, 15928, 15937, 15939, 15949, 15954, 15962, 15982,
        15984, 15988, 15995, 15996, 16026, 16046, 16050, 16066, 16078, 16083, 16112, 16121, 16127,
        16129, 16153, 16163, 16174, 16187, 16191, 16232, 16270, 16293, 16342, 16344, 16366, 16390,
        16402, 16501, 16529, 16545, 16557, 16568, 16582, 16590, 16593, 16608, 16612, 16623, 16624,
        16648, 16649, 16650, 16652, 16668, 16677, 16682, 16691, 16707, 16711, 16742, 16771, 16784,
        16785, 16789, 16811, 16831, 16847, 16860, 16875, 16923, 16924, 16925, 16955, 16972, 17018,
        17022, 17024, 17082, 17104, 17141, 17187, 17203, 17342, 17376, 17394, 17485, 17532, 17579,
        17610, 17638, 17655, 17675, 17676, 17694, 17698, 17735, 17747, 17755, 17756, 17829, 17838,
        17884, 17890, 17893, 17992, 18029, 18044, 18045, 18058, 18060, 18478, 18490, 18560, 18592,
        18647, 18717, 18722, 18818, 18891, 18957, 19035, 19036, 19075, 19076, 19162, 19183, 19261,
        19314, 19326, 19391, 19392, 19394, 19491, 19651, 19672, 19689, 19723, 19777, 19806, 19815,
        19819, 19970, 20018, 20026, 20070, 20115, 20151, 20152, 20214, 20228, 20240, 20314, 20324,
        20327, 20328, 20329, 20342, 20358, 20360, 20369, 20455, 20466, 20486, 20496, 20506, 20572,
        20583, 20627, 20642, 20666, 20687, 20729, 20749, 20750, 20752, 20762, 20764, 20766, 20777,
        20816, 20830, 20845, 20854, 20865, 20881, 20887, 20890, 20966, 21010, 21024, 21033, 21052,
        21053, 21070, 21080, 21104, 21107, 21121, 21134, 21135, 21147, 21153, 21154, 21157, 21163,
        21168, 21173, 21235, 21241, 21298, 21309, 21311, 21312, 21323, 21328, 21337, 21465, 21467,
        21527, 21538, 21541, 21543, 21582, 21605, 21673, 21681, 21689, 21690, 21700, 21722, 21730,
        21739, 21769, 21892, 21971, 22044, 22118, 22147, 22205, 22249, 22320, 22361, 22421, 22444,
        22552, 22727, 22754, 22798, 22825, 22843, 22898, 23024, 23072, 23081, 23084, 23107, 23140,
        23144, 23196, 23218, 23230, 23234, 23236, 23249, 23251, 23261, 23264, 23369, 23393, 23452,
        23453, 23467, 23468, 23469, 23538, 23580, 23608, 23646, 23712, 23733, 23765, 23770, 23781,
        23788, 23790, 23795, 23796, 23803, 23804, 23811, 23859, 23860, 23861, 23868, 23869, 23870,
        24009, 24014, 24015, 24087, 24095, 24142, 24166, 24188, 24386, 24433, 24488, 24515, 24518,
        24524, 24550, 24555, 24563, 24564, 24571, 24582, 24598, 24606, 24618, 24620, 24622, 24625,
        24633, 24646, 24668, 24681, 24691, 24695, 24709, 24714, 24723, 24755, 24789, 24794, 24804,
        24809, 24813, 24830, 24834, 24854, 24856, 24868, 24871, 24879, 24899, 24900, 24901, 24915,
        24945, 24958, 24962, 24996, 25002, 25013, 25015, 25017, 25020, 25033, 25056, 25142, 25148,
        25154, 25197, 25223, 25327, 25345, 25349, 25404, 25518, 25785, 25851, 25904, 26021, 26028,
        26088, 26170, 26327, 26360, 26424, 26465, 26598, 26611, 26616, 26718, 26826, 26968, 26970,
        26971, 27021, 27030, 27053, 27170, 27178, 27244, 27266, 27289, 27312, 27330, 27362, 27450,
        27594, 27630, 27716, 27800, 27807, 27816, 27840, 27855, 27862, 27865, 27915, 27937, 28143,
        28237, 28270, 28429, 28432, 28466, 28528, 28667, 28786, 28837, 28845, 28916, 28993, 29017,
        29031, 29034, 29039, 29049, 29066, 29072, 29141, 29261, 29302, 29313, 29324, 29416, 29631,
        29654, 29658, 29660, 29661, 29674, 29678, 29755, 29788, 29847, 29848, 29857, 29888, 29904,
        30059, 30074, 30341, 30367, 30432, 30664, 30700, 30779, 31006, 31096, 31194, 31203, 31262,
        31263, 31544, 31552, 31559, 31595, 31601, 31611, 31715, 31732, 31825, 31831, 31931, 32004,
        32059, 32061, 32072, 32075, 32077, 32082, 32086, 32092, 32096, 32115, 32125, 32137, 32180,
        32247, 32251, 32288, 32323, 32359, 32379, 32611, 32612, 32622, 32657, 32658, 32682, 32699,
        32709, 32724, 32739, 32918, 32965, 32976, 33134, 33144, 33174, 33180, 33275, 33343, 33386,
        33414, 33471, 33494, 33548, 33560, 33570, 33632, 33635, 33639, 33683, 33697, 33781, 33816,
        33841, 33855, 33928, 33958, 34078, 34120, 34239, 34261, 34262, 34445, 34487, 34506, 34546,
        34636, 34654, 34672, 34788, 34847, 34870, 34893, 34897, 34926, 34928, 34947, 35036, 35124,
        35160, 35191, 35192, 35201, 35237, 35246, 35273, 35537, 35572, 35580, 35600, 35643, 35662,
        35665, 35745, 35751, 35800, 35801, 35846, 35851, 35909, 35912, 36261, 36264, 36291, 36306,
        36713, 36799, 36820, 36994, 37013, 37296, 37488, 37690, 38138, 38295, 38562, 38651, 38715,
        38765, 38771,
    ];
    list.shuffle(&mut thread_rng());
    list
});

#[derive(Clone)]
pub struct WebpbnPuzzle {
    pub id: u32,
    pub title: Option<String>,
    pub copyright: Option<String>,
    pub rows: Vec<Vec<u8>>,
    pub columns: Vec<Vec<u8>>,
    pub solution: BitVec<usize, Lsb0>,
}

pub async fn get_random_puzzle_id() -> Result<u32> {
    let client = reqwest::ClientBuilder::new()
        .redirect(Policy::none())
        .build()
        .with_context(|| "Reqwest client build error")?;
    let redirect_response = client
        .post("https://webpbn.com/random.cgi")
        .form(&[
            ("sid", ""),
            ("go", "1"),
            ("psize", "1"),
            ("pcolor", "1"),
            ("pmulti", "1"),
            ("pguess", "1"),
            ("save", "1"),
        ])
        .send()
        .await
        .with_context(|| "URL fetch error")?;
    let location = redirect_response
        .headers()
        .get("location")
        .with_context(|| "Missing Location header")?;
    let id = location
        .to_str()
        .unwrap()
        .split_once("id=")
        .with_context(|| "Missing ID field in Location")?
        .1
        .split('&')
        .next()
        .with_context(|| "Missing ID value in Location")?
        .parse::<u32>()
        .with_context(|| "ID value is not an integer")?;
    Ok(id)
}

pub async fn get_puzzle_data(id: u32) -> Result<WebpbnPuzzle> {
    let client = reqwest::Client::new();
    let export_response = client
        .post(format!("https://webpbn.com/export.cgi/webpbn{:06}.non", id))
        .form(&[
            ("go", "1"),
            ("sid", ""),
            ("id", &id.to_string()),
            ("xml_clue", "on"),
            ("xml_soln", "on"),
            ("fmt", "ss"),
            ("ss_soln", "on"),
            ("sg_clue", "on"),
            ("sg_soln", "on"),
        ])
        .send()
        .await
        .with_context(|| "URL fetch error")?
        .text()
        .await
        .with_context(|| "Received non-text response")?;
    let mut title = None;
    let mut copyright = None;
    let mut rows = vec![];
    let mut columns = vec![];
    let mut solution = bitvec![];
    enum GetPuzzleState {
        Start,
        ReadingRows,
        ReadingColumns,
    }
    let mut state = GetPuzzleState::Start;
    for line in export_response.lines() {
        match state {
            GetPuzzleState::Start => {
                if line.starts_with("title") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().with_context(|| {
                        "Expected 'title' to be followed by double-quoted string"
                    })?;
                    title = Some(String::from(iter.next().with_context(|| {
                        "Expected 'title' to be contained within double-quoted string"
                    })?));
                } else if line.starts_with("copyright") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().with_context(|| {
                        "Expected 'copyright' to be followed by double-quoted string"
                    })?;
                    copyright = Some(String::from(iter.next().with_context(|| {
                        "Expected 'copyright' to be contained within double-quoted string"
                    })?));
                } else if line.starts_with("rows") {
                    state = GetPuzzleState::ReadingRows;
                } else if line.starts_with("columns") {
                    state = GetPuzzleState::ReadingColumns;
                } else if line.starts_with("goal") {
                    let mut iter = line.splitn(3, '"');
                    iter.next().with_context(|| {
                        "Expected 'goal' to be followed by double-quoted string"
                    })?;
                    solution.extend(
                        iter.next()
                            .with_context(|| {
                                "Expected 'goal' to be contained within double-quoted string"
                            })?
                            .chars()
                            .map(|char| char == '1'),
                    );
                }
            }
            GetPuzzleState::ReadingRows => {
                if line.is_empty() {
                    state = GetPuzzleState::Start;
                } else {
                    let row = line
                        .split(',')
                        .flat_map(|text| str::parse::<u8>(text).ok())
                        .filter(|&value| value > 0)
                        .collect::<Vec<_>>();
                    rows.push(row);
                }
            }
            GetPuzzleState::ReadingColumns => {
                if line.is_empty() {
                    state = GetPuzzleState::Start;
                } else {
                    let column = line
                        .split(',')
                        .flat_map(|text| str::parse::<u8>(text).ok())
                        .filter(|&value| value > 0)
                        .collect::<Vec<_>>();
                    columns.push(column);
                }
            }
        }
    }
    if rows.is_empty() || columns.is_empty() || solution.is_empty() {
        Err(anyhow!("Invalid puzzle"))
    } else {
        Ok(WebpbnPuzzle {
            id,
            title,
            copyright,
            rows,
            columns,
            solution,
        })
    }
}
