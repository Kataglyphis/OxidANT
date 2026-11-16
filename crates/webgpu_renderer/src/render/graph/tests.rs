use super::*;
use Resource::*;

const COLOR: TextureDesc = TextureDesc {
    size: (64, 32),
    format: wgpu::TextureFormat::Rgba16Float,
    sample_count: 1,
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING),
};

const DEPTH: TextureDesc = TextureDesc {
    format: wgpu::TextureFormat::Depth32Float,
    ..COLOR
};

fn pass<'a>(name: &'static str, reads: &'a [Resource], writes: &'a [Resource]) -> PassDesc<'a> {
    PassDesc {
        name,
        reads,
        writes,
    }
}

fn names(graph: &FrameGraph<'_>, compiled: &CompiledGraph) -> Vec<&'static str> {
    compiled
        .order
        .iter()
        .map(|&pass| graph.passes()[pass].name)
        .collect()
}

#[test]
fn the_order_follows_the_reads_not_the_order_passes_were_added_in() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("tonemap", &[HdrColor, Bloom], &[Output]));
    graph.add_pass(pass("bloom", &[HdrColor], &[Bloom]));
    graph.add_pass(pass("forward", &[], &[HdrColor]));
    graph.export(Output);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(names(&graph, &compiled), ["forward", "bloom", "tonemap"]);
}

#[test]
fn an_already_ordered_frame_keeps_its_order_among_independent_passes() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("forward", &[], &[HdrColor, Depth]));
    graph.add_pass(pass("ssao", &[Depth], &[Ambient]));
    graph.add_pass(pass("bloom", &[HdrColor], &[Bloom]));
    graph.add_pass(pass("tonemap", &[HdrColor, Bloom, Ambient], &[Output]));
    graph.export(Output);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(compiled.order, [0, 1, 2, 3]);
}

#[test]
fn a_pass_nothing_exported_reads_is_culled_with_what_only_it_needs() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("forward", &[], &[HdrColor, Depth]));
    graph.add_pass(pass("histogram", &[HdrColor], &[Histogram]));
    graph.add_pass(pass("exposure_reduce", &[Histogram], &[Exposure]));
    graph.add_pass(pass("tonemap", &[HdrColor], &[Output]));
    graph.export(Output);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(names(&graph, &compiled), ["forward", "tonemap"]);
}

#[test]
fn an_exported_side_output_keeps_its_writer() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("forward", &[], &[HdrColor, Depth]));
    graph.add_pass(pass("occlusion_cull", &[Depth], &[Visibility]));
    graph.add_pass(pass("tonemap", &[HdrColor], &[Output]));
    graph.export(Output);
    graph.export(Visibility);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(
        names(&graph, &compiled),
        ["forward", "occlusion_cull", "tonemap"]
    );
}

#[test]
fn a_cycle_is_an_error_that_names_its_passes() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("shadow", &[], &[ShadowMap]));
    graph.add_pass(pass("a", &[ShadowMap, Bloom], &[HdrColor]));
    graph.add_pass(pass("b", &[HdrColor], &[Bloom]));
    graph.export(HdrColor);
    assert_eq!(
        graph.compile(),
        Err(GraphError::Cycle {
            passes: vec!["a", "b"]
        })
    );
}

#[test]
fn reading_what_nothing_writes_is_an_error_unless_imported() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("tonemap", &[Bloom], &[Output]));
    graph.export(Output);
    assert_eq!(
        graph.compile(),
        Err(GraphError::UndefinedRead {
            pass: "tonemap",
            resource: Bloom
        })
    );
    graph.import(Bloom);
    assert!(graph.compile().is_ok());
}

#[test]
fn a_second_writer_is_an_error() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("a", &[], &[HdrColor]));
    graph.add_pass(pass("b", &[], &[HdrColor]));
    assert_eq!(
        graph.compile(),
        Err(GraphError::DuplicateWrite {
            pass: "b",
            resource: HdrColor
        })
    );
}

#[test]
fn transients_whose_lifetimes_do_not_meet_share_a_texture() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("gbuffer", &[], &[GBufferNormal, Depth]));
    graph.add_pass(pass("lighting", &[GBufferNormal, Depth], &[HdrColor]));
    graph.add_pass(pass("clouds", &[Depth], &[CloudColor]));
    graph.add_pass(pass("tonemap", &[HdrColor, CloudColor], &[Output]));
    graph.export(Output);
    for resource in [GBufferNormal, HdrColor, CloudColor] {
        graph.transient(resource, COLOR);
    }
    graph.transient(Depth, DEPTH);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(compiled.slot(GBufferNormal), compiled.slot(CloudColor));
    assert_ne!(compiled.slot(GBufferNormal), compiled.slot(HdrColor));
    assert_eq!(compiled.slots.len(), 3, "two colour slots and one depth");
    assert_eq!(
        compiled.transient_bytes(),
        2 * COLOR.bytes() + DEPTH.bytes()
    );
    assert_eq!(
        compiled.unaliased_bytes() - compiled.transient_bytes(),
        COLOR.bytes()
    );
}

#[test]
fn an_exported_transient_lives_to_the_frame_end_so_nothing_reuses_its_texture() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("gbuffer", &[], &[GBufferNormal]));
    graph.add_pass(pass("lighting", &[GBufferNormal], &[HdrColor]));
    graph.add_pass(pass("clouds", &[], &[CloudColor]));
    graph.add_pass(pass("tonemap", &[HdrColor, CloudColor], &[Output]));
    graph.export(Output);
    graph.export(GBufferNormal);
    for resource in [GBufferNormal, HdrColor, CloudColor] {
        graph.transient(resource, COLOR);
    }
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(compiled.slots.len(), 3);
}

#[test]
fn descriptions_that_differ_never_share() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("a", &[], &[GBufferNormal]));
    graph.add_pass(pass("b", &[GBufferNormal], &[HdrColor]));
    graph.add_pass(pass("c", &[HdrColor], &[CloudColor]));
    graph.add_pass(pass("d", &[CloudColor], &[Output]));
    graph.export(Output);
    graph.transient(GBufferNormal, COLOR);
    graph.transient(HdrColor, COLOR);
    let msaa = TextureDesc {
        sample_count: 4,
        ..COLOR
    };
    graph.transient(CloudColor, msaa);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(compiled.slots, [COLOR, COLOR, msaa]);
}

#[test]
fn a_transient_of_a_culled_pass_takes_no_texture() {
    let mut graph = FrameGraph::new();
    graph.add_pass(pass("forward", &[], &[HdrColor]));
    graph.add_pass(pass("unused", &[HdrColor], &[Bloom]));
    graph.add_pass(pass("tonemap", &[HdrColor], &[Output]));
    graph.export(Output);
    graph.transient(HdrColor, COLOR);
    graph.transient(Bloom, COLOR);
    let compiled = graph.compile().expect("a valid graph compiles");
    assert_eq!(compiled.slot(Bloom), None);
    assert_eq!(compiled.slots.len(), 1);
}

/// A small deterministic generator, so the random graphs are the same on every run.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n
    }
}

static POOL: [Resource; 12] = [
    HdrMsaa,
    DepthMsaa,
    Depth,
    DepthSpread,
    GBufferNormal,
    GBufferAlbedo,
    GBufferMaterial,
    HdrColor,
    Bloom,
    Ambient,
    CloudColor,
    CloudHalf,
];

/// Pass `i` writes resource `i` and reads a few written earlier, so the graph is acyclic and every read defined.
fn random_graph<'a>(rng: &mut Lcg, reads: &'a mut [Vec<Resource>; 12]) -> FrameGraph<'a> {
    for (i, read) in reads.iter_mut().enumerate() {
        read.clear();
        for _ in 0..rng.below(3) {
            if i > 0 {
                read.push(POOL[rng.below(i)]);
            }
        }
    }
    let reads: &'a [Vec<Resource>; 12] = reads;
    let mut graph = FrameGraph::new();
    for (i, read) in reads.iter().enumerate() {
        graph.add_pass(pass("p", read, std::slice::from_ref(&POOL[i])));
        let desc = if rng.below(3) == 0 { DEPTH } else { COLOR };
        graph.transient(POOL[i], desc);
    }
    graph.export(POOL[11]);
    if rng.below(2) == 0 {
        graph.export(POOL[rng.below(12)]);
    }
    graph
}

#[test]
fn aliased_transients_never_live_at_the_same_time() {
    let mut rng = Lcg(0x5eed);
    let mut reads: [Vec<Resource>; 12] = Default::default();
    let mut aliased = 0;
    for _ in 0..2000 {
        let graph = random_graph(&mut rng, &mut reads);
        let compiled = graph.compile().expect("an acyclic graph compiles");
        for (i, a) in compiled.transients.iter().enumerate() {
            for b in &compiled.transients[i + 1..] {
                if a.slot != b.slot {
                    continue;
                }
                aliased += 1;
                assert_eq!(a.desc, b.desc, "{a:?} and {b:?} share a slot");
                assert!(!a.lifetime.overlaps(b.lifetime), "{a:?} overlaps {b:?}");
            }
        }
        for transient in &compiled.transients {
            assert_eq!(compiled.slots[transient.slot], transient.desc);
        }
    }
    assert!(aliased > 1000, "the random graphs must exercise aliasing");
}

#[test]
fn every_running_pass_follows_the_writers_of_what_it_reads() {
    let mut rng = Lcg(0xfeed);
    let mut reads: [Vec<Resource>; 12] = Default::default();
    for _ in 0..500 {
        let graph = random_graph(&mut rng, &mut reads);
        let compiled = graph.compile().expect("an acyclic graph compiles");
        let position = |resource: &Resource| {
            let writer = graph
                .passes()
                .iter()
                .position(|p| p.writes.contains(resource));
            compiled.order.iter().position(|&pass| Some(pass) == writer)
        };
        for (at, &pass) in compiled.order.iter().enumerate() {
            for read in graph.passes()[pass].reads {
                let writer = position(read).expect("a running pass's writers run");
                assert!(writer < at);
            }
        }
    }
}
