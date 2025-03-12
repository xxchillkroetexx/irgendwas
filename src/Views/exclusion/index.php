<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1>Manage Exclusion Rules</h1>
        <p class="lead">For group: <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a></p>
    </div>
    <div>
        <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary">Back to Group</a>
    </div>
</div>

<div class="row">
    <div class="col-md-6">
        <div class="card mb-4">
            <div class="card-header">
                <h5 class="card-title mb-0">Add New Exclusion</h5>
            </div>
            <div class="card-body">
                <p class="text-muted">
                    Exclusion rules prevent you from being assigned to specific people during the Secret Santa draw.
                </p>

                <form action="/exclusions/<?= $group->getId() ?>/add" method="post">
                    <div class="mb-3">
                        <label for="excludedUserId" class="form-label">Select person to exclude:</label>
                        <select class="form-select" id="excludedUserId" name="excluded_user_id" required>
                            <option value="">-- Select a person --</option>
                            <?php foreach ($members as $member): ?>
                                <?php if ($member->getId() !== $auth->userId()): ?>
                                    <?php
                                    // Check if already excluded
                                    $isExcluded = false;
                                    foreach ($exclusions as $exclusion) {
                                        if ($exclusion->getExcludedUserId() === $member->getId()) {
                                            $isExcluded = true;
                                            break;
                                        }
                                    }

                                    if (!$isExcluded):
                                    ?>
                                        <option value="<?= $member->getId() ?>"><?= htmlspecialchars($member->getName()) ?></option>
                                    <?php endif; ?>
                                <?php endif; ?>
                            <?php endforeach; ?>
                        </select>
                    </div>

                    <button type="submit" class="btn btn-primary">Add Exclusion</button>
                </form>
            </div>
        </div>

        <div class="card">
            <div class="card-header">
                <h5 class="card-title mb-0">Why Use Exclusions?</h5>
            </div>
            <div class="card-body">
                <p>Exclusion rules are useful when:</p>
                <ul>
                    <li>You want to avoid being matched with your spouse or partner</li>
                    <li>You've already exchanged gifts with someone outside the Secret Santa</li>
                    <li>You want to increase the chance of meeting new people</li>
                </ul>
                <p class="text-muted small">
                    <strong>Note:</strong> Adding too many exclusions might make it impossible to find a valid
                    Secret Santa arrangement. Use them sparingly!
                </p>
            </div>
        </div>
    </div>

    <div class="col-md-6">
        <div class="card">
            <div class="card-header">
                <h5 class="card-title mb-0">Your Current Exclusions</h5>
            </div>
            <div class="card-body">
                <?php if (empty($exclusions)): ?>
                    <p class="text-center py-4">You haven't added any exclusions yet.</p>
                <?php else: ?>
                    <ul class="list-group">
                        <?php foreach ($exclusions as $exclusion): ?>
                            <li class="list-group-item d-flex justify-content-between align-items-center">
                                <span>
                                    <i class="bi bi-person-x me-2"></i>
                                    <?= htmlspecialchars($exclusion->getExcludedUser()->getName()) ?>
                                </span>
                                <a href="/exclusions/<?= $group->getId() ?>/remove/<?= $exclusion->getExcludedUserId() ?>"
                                    class="btn btn-sm btn-outline-danger"
                                    onclick="return confirm('Are you sure you want to remove this exclusion?')">
                                    Remove
                                </a>
                            </li>
                        <?php endforeach; ?>
                    </ul>
                <?php endif; ?>
            </div>
        </div>
    </div>
</div>