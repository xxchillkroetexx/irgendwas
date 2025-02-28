<div class="row mb-4">
    <div class="col">
        <h1>My Groups</h1>
    </div>
    <div class="col-auto">
        <a href="/groups/create" class="btn btn-primary">Create New Group</a>
        <a href="/groups/join" class="btn btn-outline-primary">Join a Group</a>
    </div>
</div>

<?php if (empty($groups)): ?>
    <div class="alert alert-info">
        <p class="mb-0">You are not a member of any Secret Santa groups yet.</p>
        <p class="mb-0">Create a new group or join an existing one to get started.</p>
    </div>
<?php else: ?>
    <div class="row row-cols-1 row-cols-md-2 row-cols-lg-3 g-4">
        <?php foreach ($groups as $group): ?>
            <div class="col">
                <div class="card h-100">
                    <div class="card-header">
                        <?php if ($group->getAdminId() === $auth->userId()): ?>
                            <span class="badge bg-primary float-end">Admin</span>
                        <?php endif; ?>
                        <h5 class="card-title mb-0"><?= htmlspecialchars($group->getName()) ?></h5>
                    </div>
                    <div class="card-body">
                        <?php if ($group->getDescription()): ?>
                            <p class="card-text"><?= htmlspecialchars($group->getDescription()) ?></p>
                        <?php else: ?>
                            <p class="card-text text-muted"><em>No description</em></p>
                        <?php endif; ?>
                        
                        <div class="d-flex justify-content-between align-items-center mt-3">
                            <small class="text-muted">
                                <?php if ($group->isDrawn()): ?>
                                    <span class="badge bg-success">Drawn</span>
                                <?php else: ?>
                                    <span class="badge bg-warning text-dark">Not Drawn</span>
                                <?php endif; ?>
                            </small>
                            <a href="/groups/<?= $group->getId() ?>" class="btn btn-sm btn-outline-primary">View Details</a>
                        </div>
                    </div>
                    <div class="card-footer text-muted">
                        <small>Admin: <?= htmlspecialchars($group->getAdmin() ? $group->getAdmin()->getName() : 'Unknown') ?></small>
                    </div>
                </div>
            </div>
        <?php endforeach; ?>
    </div>
<?php endif; ?>