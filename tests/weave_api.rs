use prism::{RawMesh, Shape, Tesselator};
use weave::iter::{IntoParallelIterator, ParallelIterator, ParallelSlice};
use weave::{Priority, ThreadPoolBuilder, WorkerLocal};

fn quad(x: f32) -> Shape {
    Shape::Quad {
        pos: [x, x * 0.5],
        size: [10.0, 5.0],
        rotation: 0.0,
        color: [0.2, 0.4, 0.8, 1.0],
        uv: None,
    }
}

fn tesselate(shape: &Shape) -> RawMesh {
    let mut mesh = RawMesh::default();
    Tesselator::tesselate(shape, &mut mesh);
    mesh
}

#[test]
fn prism_tesselates_shapes_through_parallel_iterators() {
    let pool = ThreadPoolBuilder::new().num_threads(4).build();
    let shapes: Vec<_> = (0..4_096).map(|index| quad(index as f32)).collect();

    let meshes: Vec<RawMesh> = pool.install(|| {
        shapes
            .iter_parallel()
            .map(tesselate)
            .filter(|mesh| !mesh.vertices().is_empty())
            .collect()
    });

    assert_eq!(meshes.len(), shapes.len());
    assert!(meshes.iter().all(|mesh| mesh.vertices().len() == 4));
    assert!(meshes.iter().all(|mesh| mesh.indices().len() == 6));
}

#[test]
fn prism_can_fill_an_existing_mesh_buffer_in_parallel() {
    let pool = ThreadPoolBuilder::new().num_threads(4).build();
    let shapes: Vec<_> = (0..2_048).map(|index| quad(index as f32)).collect();
    let mut meshes = vec![RawMesh::default(); shapes.len()];

    pool.install(|| {
        shapes.iter_parallel().map(tesselate).fill(&mut meshes);
    });

    assert!(meshes.iter().all(|mesh| mesh.vertices().len() == 4));
}

#[test]
fn prism_can_use_structured_tasks_with_borrowed_meshes() {
    let pool = ThreadPoolBuilder::new().num_threads(2).build();
    let mut left = RawMesh::default();
    let mut right = RawMesh::default();

    pool.scope(|scope| {
        scope.spawn(|| {
            Tesselator::tesselate(&quad(1.0), &mut left);
        });
        let right_task = scope.submit(|| Tesselator::tesselate(&quad(2.0), &mut right));
        right_task.join().expect("scoped task should succeed");
    });

    assert_eq!(left.vertices().len(), 4);
    assert_eq!(right.vertices().len(), 4);
}

#[test]
fn prism_observes_results_priorities_and_panics() {
    let pool = ThreadPoolBuilder::new().num_threads(2).build();
    let value = pool
        .submit_with_priority(Priority::High, || 40 + 2)
        .join()
        .expect("priority task should succeed");
    assert_eq!(value, 42);

    let panic = pool
        .submit(|| panic!("expected integration-test panic"))
        .join();
    assert!(panic.is_err());

    assert_eq!(pool.submit(|| 7).join().unwrap(), 7);
}

#[test]
fn prism_can_accumulate_per_worker_statistics() {
    let pool = ThreadPoolBuilder::new().num_threads(4).build();
    let counters = WorkerLocal::new(&pool, || 0usize);

    pool.install(|| {
        (0..10_000usize).parallelize().for_each(|_| {
            counters.with(|counter| *counter += 1);
        });
    });

    let total = counters.into_inner().into_iter().sum::<usize>();
    assert_eq!(total, 10_000);
}
