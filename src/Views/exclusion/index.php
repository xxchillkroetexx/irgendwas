<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1><?= t('exclusion.title') ?></h1>
        <p class="lead"><?= t('exclusion.forGroup') ?> <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a></p>
    </div>
    <div>
        <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary"><?= t('exclusion.backToGroup') ?></a>
    </div>
</div>

<div class="row">
    <div class="col-md-6">
        <div class="card mb-4">
            <div class="card-header">
                <h5 class="card-title mb-0"><?= t('exclusion.addNew.title') ?></h5>
            </div>
            <div class="card-body">
                <p class="text-muted">
                    <?= t('exclusion.addNew.description') ?>
                </p>

                <form action="/exclusions/<?= $group->getId() ?>/add" method="post">
                    <div class="mb-3">
                        <label for="excludedUserId" class="form-label"><?= t('exclusion.addNew.selectPerson') ?></label>
                        <select class="form-select" id="excludedUserId" name="excluded_user_id" required>
                            <option value=""><?= t('exclusion.addNew.selectPlaceholder') ?></option>
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

                    <button type="submit" class="btn btn-primary"><?= t('exclusion.addNew.submit') ?></button>
                </form>
            </div>
        </div>

        <div class="card">
            <div class="card-header">
                <h5 class="card-title mb-0"><?= t('exclusion.why.title') ?></h5>
            </div>
            <div class="card-body">
                <p><?= t('exclusion.why.intro') ?></p>
                <ul>
                    <li><?= t('exclusion.why.reason1') ?></li>
                    <li><?= t('exclusion.why.reason2') ?></li>
                    <li><?= t('exclusion.why.reason3') ?></li>
                </ul>
                <p class="text-muted small">
                    <strong>Note:</strong> <?= t('exclusion.why.note') ?>
                </p>
            </div>
        </div>
    </div>

    <div class="col-md-6">
        <div class="card">
            <div class="card-header">
                <h5 class="card-title mb-0"><?= t('exclusion.current.title') ?></h5>
            </div>
            <div class="card-body">
                <?php if (empty($exclusions)): ?>
                    <p class="text-center py-4"><?= t('exclusion.current.empty') ?></p>
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
                                    onclick="return confirm('<?= t('exclusion.current.removeConfirm') ?>')">
                                    <?= t('exclusion.current.remove') ?>
                                </a>
                            </li>
                        <?php endforeach; ?>
                    </ul>
                <?php endif; ?>
            </div>
        </div>
    </div>
</div>