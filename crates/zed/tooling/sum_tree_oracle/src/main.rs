use std::io::{self, BufRead};
use sum_tree::{Bias, ContextLessSummary, Item, SumTree};

#[derive(Clone, Debug)]
struct Number(u32);

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct Count(usize);
impl ContextLessSummary for Count {
    fn zero() -> Self {
        Self(0)
    }
    fn add_summary(&mut self, summary: &Self) {
        self.0 += summary.0;
    }
}
impl Item for Number {
    type Summary = Count;
    fn summary(&self, _: ()) -> Count {
        Count(1)
    }
}

fn main() {
    let mut tree = SumTree::<Number>::default();
    let mut map = std::collections::BTreeMap::<u32, u32>::new();
    let mut set = std::collections::BTreeSet::<u32>::new();
    for line in io::stdin().lock().lines() {
        let line = line.expect("read trace");
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        match fields[0] {
            "push" => tree.push(Number(parse(fields[1])), ()),
            "append" => {
                let values = fields[1..]
                    .iter()
                    .map(|v| Number(parse(v)))
                    .collect::<Vec<_>>();
                tree.append(SumTree::from_iter(values, ()), ());
            }
            "seek" => {
                let target = Count(parse(fields[1]));
                let bias = if fields[2] == "L" {
                    Bias::Left
                } else {
                    Bias::Right
                };
                let (start, end, item) = tree.find::<Count, _>((), &target, bias);
                println!(
                    "seek {} {} {}",
                    start.0,
                    end.0,
                    item.map_or(-1, |v| v.0 as i64)
                );
            }
            "slice" => {
                let start = parse(fields[1]);
                let end = parse(fields[2]);
                let mut cursor = tree.cursor::<Count>(());
                cursor.seek(&Count(start), Bias::Right);
                let slice = cursor.slice(&Count(end), Bias::Right);
                println!("slice {}", csv(slice.iter().map(|v| v.0)));
            }
            "map_put" => {
                map.insert(parse(fields[1]), parse(fields[2]));
            }
            "map_remove" => {
                map.remove(&parse(fields[1]));
            }
            "set_add" => {
                set.insert(parse(fields[1]));
            }
            "set_remove" => {
                set.remove(&parse(fields[1]));
            }
            "emit" => println!(
                "state {} | {} | {}",
                csv(tree.iter().map(|v| v.0)),
                pairs(&map),
                csv(set.iter().copied())
            ),
            other => panic!("unknown operation {other}"),
        }
    }
}

fn parse<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().unwrap()
}
fn csv(values: impl IntoIterator<Item = u32>) -> String {
    values
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
fn pairs(values: &std::collections::BTreeMap<u32, u32>) -> String {
    values
        .iter()
        .map(|(k, v)| format!("{k}:{v}"))
        .collect::<Vec<_>>()
        .join(",")
}
