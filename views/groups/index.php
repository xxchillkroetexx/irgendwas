<div class="d-flex justify-content-between align-items-center mb-4">
    <h1>My Secret Santa Groups</h1>
    <a href="/groups/new" class="btn btn-danger">
        <i class="fas fa-plus me-2"></i> Create New Group
    </a>
</div>

<?php if (empty($adminGroups) && empty($groups)): ?>
    <div class="alert alert-info">
        <h4>You're not in any Secret Santa groups yet</h4>
        <p>Start by creating a group or wait for an invitation.</p>
    </div>
<?php else: ?>

    <?php if (!empty($adminGroups)): ?>
    <div class="card mb-4">
        <div class="card-header">
            <h3>Groups You Manage</h3>
        </div>
        <div class="card-body p-0">
            <div class="table-responsive">
                <table class="table table-hover mb-0">
                    <thead class="table-light">
                        <tr>
                            <th>Name</th>
                            <th>Members</th>
                            <th>Join Deadline</th>
                            <th>Draw Date</th>
                            <th>Status</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        <?php foreach ($adminGroups as $group): ?>
                            <tr>
                                <td>
                                    <strong><?= htmlspecialchars($group->getName()) ?></strong>
                                    <?php if ($group->getDescription()): ?>
                                        <div class="small text-muted"><?= htmlspecialchars($group->getDescription()) ?></div>
                                    <?php endif; ?>
                                </td>
                                <td><?= count($group->getMembers()) ?></td>
                                <td>
                                    <?= $group->getJoinDeadline() ? date('M j, Y', strtotime($group->getJoinDeadline())) : 'No deadline' ?>
                                </td>
                                <td>
                                    <?= $group->getDrawDate() ? date('M j, Y', strtotime($group->getDrawDate())) : 'Not set' ?>
                                </td>
                                <td>
                                    <?php if ($group->getIsDrawn()): ?>
                                        <span class="badge bg-success">Drawn</span>
                                    <?php else: ?>
                                        <span class="badge bg-warning text-dark">Pending</span>
                                    <?php endif; ?>
                                </td>
                                <td>
                                    <div class="btn-group btn-group-sm">
                                        <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-primary">Manage</a>
                                        <a href="/groups/<?= $group->getId() ?>/edit" class="btn btn-outline-secondary">Edit</a>
                                    </div>
                                </td>
                            </tr>
                        <?php endforeach; ?>
                    </tbody>
                </table>
            </div>
        </div>
    </div>
    <?php endif; ?>

    <?php 
    $participatingGroups = array_filter($groups, function($group) use ($adminGroups) {
        foreach ($adminGroups as $adminGroup) {
            if ($adminGroup->getId() === $group->getId()) {
                return false;
            }
        }
        return true;
    });

    if (!empty($participatingGroups)): 
    ?>
    <div class="card">
        <div class="card-header">
            <h3>Groups You're Participating In</h3>
        </div>
        <div class="card-body p-0">
            <div class="table-responsive">
                <table class="table table-hover mb-0">
                    <thead class="table-light">
                        <tr>
                            <th>Name</th>
                            <th>Admin</th>
                            <th>Your Assignment</th>
                            <th>Status</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody>
                        <?php foreach ($participatingGroups as $group): 
                            $admin = $group->getAdmin();
                            $assignment = null;
                            if ($group->getIsDrawn()) {
                                $assignment = $group->getUserAssignment($user->getId());
                            }
                        ?>
                            <tr>
                                <td>
                                    <strong><?= htmlspecialchars($group->getName()) ?></strong>
                                    <?php if ($group->getDescription()): ?>
                                        <div class="small text-muted"><?= htmlspecialchars($group->getDescription()) ?></div>
                                    <?php endif; ?>
                                </td>
                                <td><?= htmlspecialchars($admin->getFullName()) ?></td>
                                <td>
                                    <?php if ($assignment): ?>
                                        <a href="/groups/<?= $group->getId() ?>/wishlist/<?= $assignment->getId() ?>">
                                            <?= htmlspecialchars($assignment->getFullName()) ?>
                                        </a>
                                    <?php else: ?>
                                        <em>Not drawn yet</em>
                                    <?php endif; ?>
                                </td>
                                <td>
                                    <?php if ($group->getIsDrawn()): ?>
                                        <span class="badge bg-success">Drawn</span>
                                    <?php else: ?>
                                        <span class="badge bg-warning text-dark">Pending</span>
                                    <?php endif; ?>
                                </td>
                                <td>
                                    <div class="btn-group btn-group-sm">
                                        <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-primary">View</a>
                                        <a href="/groups/<?= $group->getId() ?>/wishlist" class="btn btn-outline-secondary">My Wishlist</a>
                                    </div>
                                </td>
                            </tr>
                        <?php endforeach; ?>
                    </tbody>
                </table>
            </div>
        </div>
    </div>
    <?php endif; ?>

<?php endif; ?>
