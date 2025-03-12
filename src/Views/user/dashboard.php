<div class="row">
    <div class="col-md-8">
        <h2>Welcome, <?= htmlspecialchars($user->getName()) ?>!</h2>
        <p class="text-muted">Your Secret Santa dashboard</p>

        <div class="row mt-4">
            <div class="col-md-6">
                <div class="card mb-4">
                    <div class="card-header bg-primary text-white">
                        <h5 class="mb-0">My Groups</h5>
                    </div>
                    <div class="card-body">
                        <?php if (empty($groups)): ?>
                            <p>You aren't a member of any Secret Santa groups yet.</p>
                            <a href="/groups/create" class="btn btn-primary">Create a Group</a>
                        <?php else: ?>
                            <ul class="list-group">
                                <?php foreach ($groups as $group): ?>
                                    <li class="list-group-item d-flex justify-content-between align-items-center">
                                        <div>
                                            <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a>
                                            <?php if ($group->isDrawn()): ?>
                                                <span class="badge bg-success rounded-pill ms-2">Drawn</span>
                                            <?php else: ?>
                                                <span class="badge bg-warning rounded-pill ms-2">Pending</span>
                                            <?php endif; ?>
                                        </div>
                                        <?php if ($group->getAdminId() === $user->getId()): ?>
                                            <span class="badge bg-secondary">Admin</span>
                                        <?php endif; ?>
                                    </li>
                                <?php endforeach; ?>
                            </ul>
                            <div class="mt-3">
                                <a href="/groups/create" class="btn btn-primary">Create a Group</a>
                                <a href="/groups" class="btn btn-outline-primary">View All Groups</a>
                            </div>
                        <?php endif; ?>
                    </div>
                </div>
            </div>

            <div class="col-md-6">
                <div class="card mb-4">
                    <div class="card-header bg-success text-white">
                        <h5 class="mb-0">My Secret Santa Assignments</h5>
                    </div>
                    <div class="card-body">
                        <?php if (empty($assignments)): ?>
                            <p>You don't have any gift assignments yet.</p>
                            <p class="text-muted">When your group performs the Secret Santa draw, your assignment will appear here.</p>
                        <?php else: ?>
                            <ul class="list-group">
                                <?php foreach ($assignments as $assignment): ?>
                                    <li class="list-group-item">
                                        <strong>Group:</strong> <?= htmlspecialchars($assignment->getGroup()->getName()) ?><br>
                                        <strong>You're giving a gift to:</strong> <?= htmlspecialchars($assignment->getReceiver()->getName()) ?><br>
                                        <a href="/wishlist/view/<?= $assignment->getReceiverId() ?>/<?= $assignment->getGroupId() ?>" class="btn btn-sm btn-outline-success mt-2">
                                            View Wishlist
                                        </a>
                                    </li>
                                <?php endforeach; ?>
                            </ul>
                        <?php endif; ?>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <div class="col-md-4">
        <div class="card mb-4">
            <div class="card-header bg-info text-white">
                <h5 class="mb-0">Quick Actions</h5>
            </div>
            <div class="card-body">
                <div class="d-grid gap-2">
                    <a href="/groups/create" class="btn btn-outline-primary">Create a Group</a>
                    <a href="/user/profile" class="btn btn-outline-secondary">Edit Profile</a>
                    <a href="/groups" class="btn btn-outline-info">Manage Groups</a>
                </div>
            </div>
        </div>

        <div class="card">
            <div class="card-header bg-light">
                <h5 class="mb-0">Tips</h5>
            </div>
            <div class="card-body">
                <ul class="list-unstyled">
                    <li class="mb-2">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check-circle-fill text-success me-2" viewBox="0 0 16 16">
                            <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z" />
                        </svg>
                        Create a wishlist for each group you join
                    </li>
                    <li class="mb-2">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check-circle-fill text-success me-2" viewBox="0 0 16 16">
                            <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z" />
                        </svg>
                        Invite friends and family using their email
                    </li>
                    <li class="mb-2">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check-circle-fill text-success me-2" viewBox="0 0 16 16">
                            <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z" />
                        </svg>
                        Set exclusion rules for your groups
                    </li>
                </ul>
            </div>
        </div>
    </div>
</div>