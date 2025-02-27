<div class="row mb-4">
    <div class="col-md-12">
        <div class="jumbotron">
            <h1 class="display-4">Welcome back, <?= $user->getFirstName() ?>!</h1>
            <p class="lead">Manage your Secret Santa groups and wishlists from this dashboard.</p>
            <hr class="my-4">
            <p>
                <a href="/groups/new" class="btn btn-danger">Create New Group</a>
            </p>
        </div>
    </div>
</div>

<?php if (empty($groups) && empty($adminGroups)): ?>
    <div class="alert alert-info">
        <h4>You're not in any Secret Santa groups yet</h4>
        <p>Start by creating a group or wait for an invitation.</p>
    </div>
<?php else: ?>

    <?php if (!empty($adminGroups)): ?>
    <div class="row mb-4">
        <div class="col-md-12">
            <div class="card">
                <div class="card-header">
                    <h3 class="mb-0">Groups You Manage</h3>
                </div>
                <div class="card-body p-0">
                    <div class="table-responsive">
                        <table class="table table-hover mb-0">
                            <thead>
                                <tr>
                                    <th>Group Name</th>
                                    <th>Members</th>
                                    <th>Join Deadline</th>
                                    <th>Draw Status</th>
                                    <th>Actions</th>
                                </tr>
                            </thead>
                            <tbody>
                                <?php foreach ($adminGroups as $group): ?>
                                    <tr>
                                        <td><?= htmlspecialchars($group->getName()) ?></td>
                                        <td><?= count($group->getMembers()) ?></td>
                                        <td>
                                            <?= $group->getJoinDeadline() ? date('M j, Y', strtotime($group->getJoinDeadline())) : 'No deadline' ?>
                                        </td>
                                        <td>
                                            <?php if ($group->getIsDrawn()): ?>
                                                <span class="badge bg-success">Drawn</span>
                                            <?php else: ?>
                                                <span class="badge bg-warning">Pending</span>
                                            <?php endif; ?>
                                        </td>
                                        <td>
                                            <a href="/groups/<?= $group->getId() ?>" class="btn btn-sm btn-primary">Manage</a>
                                        </td>
                                    </tr>
                                <?php endforeach; ?>
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    </div>
    <?php endif; ?>

    <?php if (!empty($groups)): ?>
    <div class="row mb-4">
        <div class="col-md-12">
            <div class="card">
                <div class="card-header">
                    <h3 class="mb-0">Groups You're Participating In</h3>
                </div>
                <div class="card-body p-0">
                    <div class="table-responsive">
                        <table class="table table-hover mb-0">
                            <thead>
                                <tr>
                                    <th>Group Name</th>
                                    <th>Your Assignment</th>
                                    <th>Status</th>
                                    <th>Actions</th>
                                </tr>
                            </thead>
                            <tbody>
                                <?php foreach ($groups as $group): 
                                    // Skip groups that user administers (already shown above)
                                    if ($group->isAdmin($user->getId())) continue;
                                    
                                    $assignment = null;
                                    if ($group->getIsDrawn()) {
                                        $assignment = $group->getUserAssignment($user->getId());
                                    }
                                ?>
                                    <tr>
                                        <td><?= htmlspecialchars($group->getName()) ?></td>
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
                                                <span class="badge bg-warning">Pending</span>
                                            <?php endif; ?>
                                        </td>
                                        <td>
                                            <div class="btn-group">
                                                <a href="/groups/<?= $group->getId() ?>" class="btn btn-sm btn-primary">View Group</a>
                                                <a href="/groups/<?= $group->getId() ?>/wishlist" class="btn btn-sm btn-outline-primary">My Wishlist</a>
                                            </div>
                                        </td>
                                    </tr>
                                <?php endforeach; ?>
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    </div>
    <?php endif; ?>
    
<?php endif; ?>
