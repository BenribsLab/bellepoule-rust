//! Réorganisation de la formule : monter / descendre une phase.

use bp_core::contest::Contest;
use bp_core::stage::StageKind;

fn kinds(c: &Contest) -> Vec<StageKind> {
    c.schedule.stages().map(|s| s.kind()).collect()
}

#[test]
fn pool_round_moves_with_its_allocator() {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(false);
    // Pointage, composition, poules, tableau, classement
    assert_eq!(
        kinds(&c),
        [StageKind::Checkin, StageKind::PoolAllocator, StageKind::PoolSupervisor, StageKind::TableSupervisor, StageKind::GeneralClassification]
    );
    // Ni le pointage ni le classement ne bougent.
    assert!(!c.schedule.can_move(2, true));
    assert!(!c.schedule.can_move(3, false));

    let moved = c.with_schedule(|s, _| s.move_user_stage(3, true));
    assert_eq!(moved, Some(1));
    assert_eq!(
        kinds(&c),
        [StageKind::Checkin, StageKind::TableSupervisor, StageKind::PoolAllocator, StageKind::PoolSupervisor, StageKind::GeneralClassification]
    );

    // La sélection sur la composition déplace aussi le tour de poules.
    let moved = c.with_schedule(|s, _| s.move_user_stage(2, true));
    assert_eq!(moved, Some(1));
    assert_eq!(
        kinds(&c),
        [StageKind::Checkin, StageKind::PoolAllocator, StageKind::PoolSupervisor, StageKind::TableSupervisor, StageKind::GeneralClassification]
    );

    let moved = c.with_schedule(|s, _| s.move_user_stage(2, false));
    assert_eq!(moved, Some(3));
    assert_eq!(c.schedule.stage(3).map(|s| s.kind()), Some(StageKind::PoolSupervisor));
}

#[test]
fn two_pool_rounds_keep_their_numbering() {
    let mut c = Contest::new(None, false);
    c.create_default_schedule(false);
    c.with_schedule(|s, ctx| {
        s.add_kind(ctx, StageKind::PoolSupervisor, Some(3));
    });
    let names = |c: &Contest| -> Vec<String> {
        c.schedule.stages().filter(|s| s.kind() == StageKind::PoolSupervisor).map(|s| s.name()).collect()
    };
    let before = names(&c);
    assert_eq!(before.len(), 2);
    // Tableau entre les deux tours, puis remonté devant le premier.
    let table = c.schedule.stages().position(|s| s.kind() == StageKind::TableSupervisor).unwrap();
    assert!(c.with_schedule(|s, _| s.move_user_stage(table, true)).is_some());
    assert_eq!(names(&c), before);
}
