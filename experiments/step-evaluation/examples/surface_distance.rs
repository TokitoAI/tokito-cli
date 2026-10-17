//! Independent development-only bounded sampled mesh distance. No admission.
#![forbid(unsafe_code)]
use serde_json::json;
use std::{fs::File, io::Read};
type P = [f64; 3];
const MAX_BYTES: usize = 96 * 1024 * 1024;
const MAX_TRIANGLES: usize = 500_000;
const MAX_VERTICES: usize = 300_000;
const MAX_EVALUATIONS: usize = 50_000_000;
struct Mesh {
    points: Vec<P>,
    triangles: Vec<[usize; 3]>,
}
struct Input {
    a: Mesh,
    b: Mesh,
}
fn parse_mesh(value: &serde_json::Value) -> Result<Mesh, String> {
    let obj = value.as_object().ok_or("mesh_object_required")?;
    if obj.len() != 2 || !obj.contains_key("points") || !obj.contains_key("triangles") {
        return Err("mesh_keys".into());
    }
    let p = value["points"].as_array().ok_or("points_array")?;
    let t = value["triangles"].as_array().ok_or("triangles_array")?;
    if p.len() > MAX_VERTICES || t.len() > MAX_TRIANGLES {
        return Err("mesh_limit".into());
    }
    let mut points = Vec::new();
    let mut triangles = Vec::new();
    for point in p {
        let row = point
            .as_array()
            .filter(|r| r.len() == 3)
            .ok_or("point_shape")?;
        points.push([
            row[0].as_f64().ok_or("point_number")?,
            row[1].as_f64().ok_or("point_number")?,
            row[2].as_f64().ok_or("point_number")?,
        ]);
    }
    for triangle in t {
        let row = triangle
            .as_array()
            .filter(|r| r.len() == 3)
            .ok_or("triangle_shape")?;
        let mut indices = [0; 3];
        for i in 0..3 {
            indices[i] = usize::try_from(row[i].as_u64().ok_or("triangle_index")?)
                .map_err(|_| "triangle_index_overflow")?;
        }
        triangles.push(indices);
    }
    Ok(Mesh { points, triangles })
}
fn sub(a: P, b: P) -> P {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: P, b: P) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn length(p: P) -> f64 {
    dot(p, p).sqrt()
}
fn segment(p: P, a: P, b: P) -> f64 {
    let d = sub(b, a);
    let denominator = dot(d, d);
    let t = if denominator > 0. {
        (dot(sub(p, a), d) / denominator).clamp(0., 1.)
    } else {
        0.
    };
    length(sub(p, std::array::from_fn(|i| a[i] + t * d[i])))
}
fn distance(p: P, a: P, b: P, c: P) -> f64 {
    // Project onto the triangle plane; retain the projection only inside all
    // three consistently oriented edge half-spaces. Otherwise use its edges.
    let ab = sub(b, a);
    let ac = sub(c, a);
    let cross = |x: P, y: P| {
        [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ]
    };
    let n = cross(ab, ac);
    let denominator = dot(n, n);
    if denominator > 0. && denominator.is_finite() {
        let signed = dot(sub(p, a), n) / denominator;
        let q = std::array::from_fn(|i| p[i] - signed * n[i]);
        if [(a, b), (b, c), (c, a)]
            .iter()
            .all(|(x, y)| dot(cross(sub(*y, *x), sub(q, *x)), n) >= 0.)
        {
            return length(sub(p, q));
        }
    }
    segment(p, a, b).min(segment(p, b, c)).min(segment(p, c, a))
}
struct Node {
    lo: P,
    hi: P,
    children: Option<(usize, usize)>,
    start: usize,
    end: usize,
}
struct Tree<'a> {
    mesh: &'a Mesh,
    order: Vec<usize>,
    nodes: Vec<Node>,
}
impl<'a> Tree<'a> {
    fn new(mesh: &'a Mesh) -> Result<Self, String> {
        if mesh.points.is_empty()
            || mesh.points.len() > MAX_VERTICES
            || mesh.triangles.is_empty()
            || mesh.triangles.len() > MAX_TRIANGLES
        {
            return Err("mesh_limit".into());
        }
        if mesh
            .points
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e12)
            || mesh
                .triangles
                .iter()
                .flatten()
                .any(|i| *i >= mesh.points.len())
        {
            return Err("invalid_mesh".into());
        }
        let mut tree = Self {
            mesh,
            order: (0..mesh.triangles.len()).collect(),
            nodes: Vec::new(),
        };
        tree.build(0, tree.order.len(), 0)?;
        Ok(tree)
    }
    fn center(&self, index: usize, axis: usize) -> f64 {
        self.mesh.triangles[index]
            .iter()
            .map(|i| self.mesh.points[*i][axis] / 3.)
            .sum()
    }
    fn build(&mut self, start: usize, end: usize, depth: usize) -> Result<usize, String> {
        if depth > 32 {
            return Err("tree_depth_limit".into());
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for i in &self.order[start..end] {
            for v in self.mesh.triangles[*i] {
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(self.mesh.points[v][axis]);
                    hi[axis] = hi[axis].max(self.mesh.points[v][axis]);
                }
            }
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            lo,
            hi,
            children: None,
            start,
            end,
        });
        if end - start > 12 {
            let axis = (0..3)
                .max_by(|a, b| (hi[*a] - lo[*a]).total_cmp(&(hi[*b] - lo[*b])))
                .unwrap();
            let mut sorted = self.order[start..end].to_vec();
            sorted.sort_unstable_by(|a, b| {
                self.center(*a, axis)
                    .total_cmp(&self.center(*b, axis))
                    .then(a.cmp(b))
            });
            self.order[start..end].copy_from_slice(&sorted);
            let middle = start + (end - start) / 2;
            let left = self.build(start, middle, depth + 1)?;
            let right = self.build(middle, end, depth + 1)?;
            self.nodes[id].children = Some((left, right));
        }
        Ok(id)
    }
    fn box_squared(&self, p: P, index: usize) -> f64 {
        let n = &self.nodes[index];
        (0..3)
            .map(|i| {
                let d = if p[i] < n.lo[i] {
                    n.lo[i] - p[i]
                } else if p[i] > n.hi[i] {
                    p[i] - n.hi[i]
                } else {
                    0.
                };
                d * d
            })
            .sum()
    }
    fn nearest(&self, p: P, work: &mut usize) -> Result<f64, String> {
        let mut pending = vec![0];
        let mut best = f64::INFINITY;
        while let Some(id) = pending.pop() {
            if self.box_squared(p, id) > best * best {
                continue;
            }
            let node = &self.nodes[id];
            if let Some((a, b)) = node.children {
                if self.box_squared(p, a) < self.box_squared(p, b) {
                    pending.extend([b, a]);
                } else {
                    pending.extend([a, b]);
                }
            } else {
                for i in &self.order[node.start..node.end] {
                    // A leaf box encloses several triangles. Reject each
                    // triangle's own box before spending a distance evaluation.
                    let vertices = self.mesh.triangles[*i].map(|v| self.mesh.points[v]);
                    let bound = (0..3)
                        .map(|axis| {
                            let lo = vertices
                                .iter()
                                .map(|v| v[axis])
                                .fold(f64::INFINITY, f64::min);
                            let hi = vertices
                                .iter()
                                .map(|v| v[axis])
                                .fold(f64::NEG_INFINITY, f64::max);
                            let delta = if p[axis] < lo {
                                lo - p[axis]
                            } else if p[axis] > hi {
                                p[axis] - hi
                            } else {
                                0.
                            };
                            delta * delta
                        })
                        .sum::<f64>();
                    if bound > best * best {
                        continue;
                    }
                    *work += 1;
                    if *work > MAX_EVALUATIONS {
                        return Err("triangle_distance_budget".into());
                    }
                    let [a, b, c] = vertices;
                    let value = distance(p, a, b, c);
                    if !value.is_finite() {
                        return Err("nonfinite_distance".into());
                    }
                    best = best.min(value);
                    // All distances are nonnegative. An exact zero is the
                    // global lower bound; no remaining node can improve it.
                    if best == 0. {
                        return Ok(0.);
                    }
                }
            }
        }
        Ok(best)
    }
}
fn directed(source: &Mesh, target: &Tree, work: &mut usize) -> Result<serde_json::Value, String> {
    let count = source.triangles.len().min(75_000);
    let mut cache = std::collections::BTreeMap::new();
    let mut maximum: f64 = 0.;
    let mut sum = 0.;
    for sample in 0..count {
        let index = if source.triangles.len() <= 75_000 {
            sample
        } else {
            sample * source.triangles.len() / 75_000
        };
        let [a, b, c] = source.triangles[index].map(|i| source.points[i]);
        let center = std::array::from_fn(|i| (a[i] + b[i] + c[i]) / 3.);
        for p in [a, b, c, center] {
            let key = p.map(|v| if v == 0. { 0 } else { v.to_bits() });
            let value = if let Some(value) = cache.get(&key) {
                *value
            } else {
                if cache.len() >= 300_000 {
                    return Err("query_cache_limit".into());
                }
                let value = target.nearest(p, work)?;
                cache.insert(key, value);
                value
            };
            maximum = maximum.max(value);
            sum += value;
        }
    }
    Ok(json!({"maximum":maximum,"mean":sum/(count*4) as f64,"sampleCount":count*4}))
}
fn compare(input: &Input) -> Result<serde_json::Value, String> {
    let a = Tree::new(&input.a)?;
    let b = Tree::new(&input.b)?;
    let mut work = 0;
    let forward = directed(&input.a, &b, &mut work)?;
    let reverse = directed(&input.b, &a, &mut work)?;
    Ok(
        json!({"schema":"tokito.independent-sampled-distance.v1","forward":forward,"reverse":reverse,"triangleDistanceEvaluations":work,"admitted":false}),
    )
}
fn main() {
    let result = (|| -> Result<serde_json::Value, String> {
        let args = std::env::args_os().skip(1).collect::<Vec<_>>();
        if args.len() != 1 {
            return Err("one_explicit_input_required".into());
        }
        let mut bytes = Vec::new();
        File::open(&args[0])
            .map_err(|e| e.to_string())?
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("input_byte_limit".into());
        }
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if value
            .as_object()
            .is_none_or(|o| o.len() != 2 || !o.contains_key("a") || !o.contains_key("b"))
        {
            return Err("input_keys".into());
        }
        compare(&Input {
            a: parse_mesh(&value["a"])?,
            b: parse_mesh(&value["b"])?,
        })
    })();
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => {
            println!("{}", json!({"error":error,"admitted":false}));
            std::process::exit(2);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branching_tree_matches_full_scan_for_offset_and_coplanar_points() {
        let mut mesh = Mesh {
            points: Vec::new(),
            triangles: Vec::new(),
        };
        for i in 0..64 {
            let x = (i % 8) as f64;
            let y = (i / 8) as f64;
            let z = (i % 3) as f64 * 0.125;
            let start = mesh.points.len();
            mesh.points
                .extend([[x, y, z], [x + 0.75, y, z], [x, y + 0.5, z]]);
            mesh.triangles.push([start, start + 1, start + 2]);
        }
        let tree = Tree::new(&mesh).unwrap();
        assert!(tree.nodes.len() > 1);
        for i in 0..1000 {
            let p = [
                (i % 79) as f64 / 9. - 1.,
                (i % 83) as f64 / 11. - 1.,
                (i % 13) as f64 / 7. - 0.5,
            ];
            let expected = mesh
                .triangles
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|v| mesh.points[v]);
                    distance(p, a, b, c)
                })
                .fold(f64::INFINITY, f64::min);
            assert!((tree.nearest(p, &mut 0).unwrap() - expected).abs() < 1e-12);
        }
        let mut work = 0;
        assert_eq!(tree.nearest(mesh.points[0], &mut work).unwrap(), 0.);
        assert!(work < mesh.triangles.len());
    }
    #[test]
    fn analytic_triangle_and_segment_cases() {
        let a = [0., 0., 0.];
        let b = [1., 0., 0.];
        let c = [0., 1., 0.];
        assert_eq!(distance([0.25, 0.25, 2.], a, b, c), 2.);
        assert_eq!(distance([2., 0., 0.], a, b, c), 1.);
        assert_eq!(distance([0., 0., 0.], a, b, c), 0.);
        assert_eq!(distance([0., 1., 0.], a, a, b), 1.);
    }
    #[test]
    fn tree_matches_bruteforce_nonzero_queries_and_refuses_invalid() {
        let mesh = Mesh {
            points: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            triangles: vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
        };
        let tree = Tree::new(&mesh).unwrap();
        for i in 0..1000 {
            let p = [
                i as f64 / 137. - 2.,
                i as f64 / 317. - 1.,
                i as f64 / 211. - 3.,
            ];
            let expected = mesh
                .triangles
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|v| mesh.points[v]);
                    distance(p, a, b, c)
                })
                .fold(f64::INFINITY, f64::min);
            assert_eq!(tree.nearest(p, &mut 0).unwrap(), expected);
        }
        let mut exhausted = MAX_EVALUATIONS;
        assert!(tree.nearest([2., 2., 2.], &mut exhausted).is_err());
        assert!(Tree::new(&Mesh {
            points: vec![[f64::NAN, 0., 0.]],
            triangles: vec![[0, 0, 0]]
        })
        .is_err());
        assert!(Tree::new(&Mesh {
            points: vec![[0.; 3]],
            triangles: vec![[0, 0, 1]]
        })
        .is_err());
    }
}
