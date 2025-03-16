<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1><?= htmlspecialchars($group->getName()) ?></h1>
        <?php if ($group->getDescription()): ?>
            <p class="lead"><?= htmlspecialchars($group->getDescription()) ?></p>
        <?php endif; ?>
    </div>
    <div>
        <?php if ($is_admin): ?>
            <div class="btn-group">
                <a href="/groups/<?= $group->getId() ?>/edit" class="btn btn-outline-primary"><?= t('group.show.editButton') ?></a>
                <button type="button" class="btn btn-outline-primary dropdown-toggle dropdown-toggle-split" data-bs-toggle="dropdown" aria-expanded="false">
                    <span class="visually-hidden"><?= t('common.toggleDropdown') ?></span>
                </button>
                <ul class="dropdown-menu dropdown-menu-end">
                    <li><a class="dropdown-item" href="/groups/<?= $group->getId() ?>/regenerate-invitation"><?= t('group.show.regenerateCode') ?></a></li>
                    <?php if (!$group->isDrawn()): ?>
                        <li><a class="dropdown-item" href="/groups/<?= $group->getId() ?>/draw" onclick="return confirm('<?= t('group.show.performDrawConfirm') ?>')"><?= t('group.show.performDraw') ?></a></li>
                    <?php endif; ?>
                    <li>
                        <hr class="dropdown-divider">
                    </li>
                    <li><a class="dropdown-item text-danger" href="/groups/<?= $group->getId() ?>/delete" onclick="return confirm('<?= t('group.show.deleteGroupConfirm') ?>')"><?= t('group.show.deleteGroup') ?></a></li>
                </ul>
            </div>
        <?php else: ?>
            <a href="/groups/<?= $group->getId() ?>/leave" class="btn btn-outline-danger" onclick="return confirm('<?= t('group.show.leaveGroupConfirm') ?>')"><?= t('group.show.leaveGroup') ?></a>
        <?php endif; ?>
    </div>
</div>

<!-- Status and details -->
<div class="row mb-4">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h5 class="card-title mb-0"><?= t('group.show.details') ?></h5>
                <?php if ($is_admin && !$group->isDrawn() && !empty($group->getMembers()) && count($group->getMembers()) >= 3): ?>
                    <a href="/groups/<?= $group->getId() ?>/draw" class="btn btn-sm btn-success"
                        onclick="return confirm('<?= t('group.show.performDrawConfirm') ?>')">
                        <?= t('group.show.performDraw') ?>
                    </a>
                <?php endif; ?>
            </div>
            <div class="card-body">
                <ul class="list-group list-group-flush">
                    <li class="list-group-item d-flex justify-content-between align-items-center">
                        <span><?= t('group.show.status') ?>:</span>
                        <?php if ($group->isDrawn()): ?>
                            <span class="badge rounded-pill bg-success"><?= t('group.status.drawn') ?></span>
                        <?php else: ?>
                            <span class="badge rounded-pill bg-warning text-dark"><?= t('group.status.notDrawn') ?></span>
                        <?php endif; ?>
                    </li>
                    <li class="list-group-item d-flex justify-content-between align-items-center">
                        <span><?= t('group.adminLabel') ?>:</span>
                        <span><?= htmlspecialchars($group->getAdmin() ? $group->getAdmin()->getName() : 'Unknown') ?></span>
                    </li>
                    <?php if ($group->getRegistrationDeadline()): ?>
                        <li class="list-group-item d-flex justify-content-between align-items-center">
                            <span><?= t('group.show.registrationDeadline') ?>:</span>
                            <span><?= date('F j, Y', strtotime($group->getRegistrationDeadline())) ?></span>
                        </li>
                    <?php endif; ?>
                    <?php if ($group->getDrawDate()): ?>
                        <li class="list-group-item d-flex justify-content-between align-items-center">
                            <span><?= t('group.show.drawDate') ?>:</span>
                            <span><?= date('F j, Y', strtotime($group->getDrawDate())) ?></span>
                        </li>
                    <?php endif; ?>
                    <?php if ($group->getCreatedAt()): ?>
                        <li class="list-group-item d-flex justify-content-between align-items-center">
                            <span><?= t('group.show.createdOn') ?>:</span>
                            <span><?= date('F j, Y', strtotime($group->getCreatedAt())) ?></span>
                        </li>
                    <?php endif; ?>
                </ul>
            </div>
        </div>
    </div>

    <?php if ($is_admin): ?>
        <div class="col-md-6">
            <div class="card">
                <div class="card-header">
                    <h5 class="card-title"><?= t('group.show.invitationCode') ?></h5>
                </div>
                <div class="card-body">
                    <p class="card-text"><?= t('group.show.shareCode') ?></p>
                    <div class="input-group mb-3">
                        <input type="text" class="form-control" value="<?= htmlspecialchars($group->getInvitationCode()) ?>" readonly id="invitation-code">
                        <button class="btn btn-outline-secondary" type="button" onclick="copyToClipboard('invitation-code')"><?= t('group.show.copy') ?></button>
                    </div>
                    <p class="card-text">
                        <small class="text-muted">
                            <?= t('group.show.shareLink') ?>
                            <?php
                            $baseUrl = $_SERVER['REQUEST_SCHEME'] . '://' . $_SERVER['HTTP_HOST'];
                            $joinUrl = $baseUrl . '/groups/join?code=' . $group->getInvitationCode();
                            ?>
                            <a href="<?= $joinUrl ?>" target="_blank"><?= $joinUrl ?></a>
                        </small>
                    </p>
                </div>
            </div>
        </div>
    <?php endif; ?>
</div>

<!-- Assignment result (if drawn) -->
<?php if ($group->isDrawn() && $assignment): ?>
    <div class="row mb-4">
        <div class="col-12">
            <div class="card border-success">
                <div class="card-header bg-success text-white">
                    <h5 class="card-title mb-0"><?= t('group.show.assignment') ?></h5>
                </div>
                <div class="card-body">
                    <p class="card-text"><?= t('group.show.assignmentDescription') ?></p>
                    <h3 class="text-center mb-4"><?= htmlspecialchars($assignment->getReceiver()->getName()) ?></h3>

                    <div class="d-grid gap-2 col-md-6 mx-auto">
                        <a href="/wishlist/view/<?= $assignment->getReceiverId() ?>/<?= $group->getId() ?>" class="btn btn-primary"><?= t('group.show.viewWishlist') ?></a>
                    </div>
                </div>
            </div>
        </div>
    </div>
<?php endif; ?>

<!-- Group members -->
<div class="row mb-4">
    <div class="col-12">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h5 class="card-title mb-0"><?= t('group.show.members', ['count' => count($group->getMembers())]) ?></h5>
                <?php if (!$group->isDrawn()): ?>
                    <a href="/exclusions/<?= $group->getId() ?>" class="btn btn-sm btn-outline-primary"><?= t('group.show.manageExclusions') ?></a>
                <?php endif; ?>
            </div>
            <div class="card-body">
                <?php if (empty($group->getMembers())): ?>
                    <p class="card-text"><?= t('group.show.noMembers') ?></p>
                <?php else: ?>
                    <div class="row row-cols-1 row-cols-md-2 row-cols-lg-3 g-4">
                        <?php foreach ($group->getMembers() as $member): ?>
                            <div class="col">
                                <div class="card h-100">
                                    <div class="card-body">
                                        <h5 class="card-title">
                                            <?= htmlspecialchars($member->getName()) ?>
                                            <?php if ($member->getId() === $group->getAdminId()): ?>
                                                <span class="badge bg-primary"><?= t('group.admin') ?></span>
                                            <?php endif; ?>
                                            <?php if ($member->getId() === $auth->userId()): ?>
                                                <span class="badge bg-secondary"><?= t('group.show.you') ?></span>
                                            <?php endif; ?>
                                        </h5>
                                        <p class="card-text text-muted"><?= htmlspecialchars($member->getEmail()) ?></p>
                                    </div>
                                </div>
                            </div>
                        <?php endforeach; ?>
                    </div>
                <?php endif; ?>
            </div>
        </div>
    </div>
</div>

<!-- Your wishlist -->
<div class="row mb-4">
    <div class="col-12">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h5 class="card-title mb-0"><?= t('group.show.yourWishlist') ?></h5>
                <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-sm btn-primary"><?= t('group.show.editWishlist') ?></a>
            </div>
            <div class="card-body">
                <p class="card-text">
                    <?= t('group.show.wishlistHelp') ?>
                </p>
                <p class="card-text">
                    <a href="/wishlist/view/<?= $auth->userId() ?>/<?= $group->getId() ?>"><?= t('group.show.viewYourWishlist') ?></a> <?= t('group.show.viewYourWishlistHelp') ?>
                </p>
            </div>
        </div>
    </div>
</div>

<script>
    function copyToClipboard(elementId) {
        var input = document.getElementById(elementId);
        input.select();
        input.setSelectionRange(0, 99999);
        document.execCommand("copy");

        // Show toast or alert that copying was successful
        alert("Copied to clipboard!");
    }
</script>