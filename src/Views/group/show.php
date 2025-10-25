<?php
/**
 * Group detail view template
 * 
 * This template displays detailed information about a group including:
 * - Group name and description
 * - Admin controls for group management
 * - Group status and details
 * - Secret Santa assignment (if drawn)
 * - Member list
 * - Wishlist access
 * 
 * @var Group $group The group object being displayed
 * @var bool $is_admin Whether the current user is the admin of this group
 * @var Assignment|null $assignment The current user's assignment (if drawn)
 * @var Auth $auth Authentication instance for current user data
 */
?>

<!-- Header section with group name, description and action buttons -->
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

<!-- Status and details section showing group information -->
<div class="row mb-4">
    <div class="col-md-6">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h5 class="card-title mb-0"><?= t('group.show.details') ?></h5>
                <?php 
                /**
                 * Show draw button if user is admin, group is not drawn yet,
                 * and has at least 3 members (minimum required for draw)
                 */
                if ($is_admin && !$group->isDrawn() && !empty($group->getMembers()) && count($group->getMembers()) >= 3): ?>
                    <a href="/groups/<?= $group->getId() ?>/draw" class="btn btn-sm btn-secondary"
                        onclick="return confirm('<?= t('group.show.performDrawConfirm') ?>')">
                        <?= t('group.show.performDraw') ?>
                    </a>
                <?php endif; ?>
            </div>
            <div class="card-body">
                <ul class="list-group list-group-flush">
                    <!-- Status indicator: drawn or not drawn -->
                    <li class="list-group-item d-flex justify-content-between align-items-center">
                        <span><?= t('group.show.status') ?>:</span>
                        <?php if ($group->isDrawn()): ?>
                            <span class="badge rounded-pill bg-success"><?= t('group.status.drawn') ?></span>
                        <?php else: ?>
                            <span class="badge rounded-pill bg-warning text-dark"><?= t('group.status.notDrawn') ?></span>
                        <?php endif; ?>
                    </li>
                    <!-- Group admin information -->
                    <li class="list-group-item d-flex justify-content-between align-items-center">
                        <span><?= t('group.adminLabel') ?>:</span>
                        <span><?= htmlspecialchars($group->getAdmin() ? $group->getAdmin()->getName() : 'Unknown') ?></span>
                    </li>
                    <!-- Display registration deadline if set -->
                    <?php if ($group->getRegistrationDeadline()): ?>
                        <li class="list-group-item d-flex justify-content-between align-items-center">
                            <span><?= t('group.show.registrationDeadline') ?>:</span>
                            <span><?= date('F j, Y', strtotime($group->getRegistrationDeadline())) ?></span>
                        </li>
                    <?php endif; ?>
                    <!-- Display draw date if set -->
                    <?php if ($group->getDrawDate()): ?>
                        <li class="list-group-item d-flex justify-content-between align-items-center">
                            <span><?= t('group.show.drawDate') ?>:</span>
                            <span><?= date('F j, Y', strtotime($group->getDrawDate())) ?></span>
                        </li>
                    <?php endif; ?>
                    <!-- Display creation date -->
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

    <!-- Invitation code section (only visible to admin) -->
    <?php if ($is_admin): ?>
        <div class="col-md-6">
            <div class="card">
                <div class="card-header">
                    <h5 class="card-title"><?= t('group.show.invitationCode') ?></h5>
                </div>
                <div class="card-body">
                    <p class="card-text"><?= t('group.show.shareCode') ?></p>
                    <!-- Copy to clipboard functionality for invitation code -->
                    <div class="input-group mb-3">
                        <input type="text" class="form-control" value="<?= htmlspecialchars($group->getInvitationCode()) ?>" readonly id="invitation-code">
                        <button class="btn btn-outline-secondary" type="button" onclick="copyToClipboard('invitation-code')"><?= t('group.show.copy') ?></button>
                    </div>
                    <p class="card-text">
                        <small class="text-muted">
                            <?= t('group.show.shareLink') ?>
                            <?php
                            // Generate shareable invitation link
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

<!-- Assignment result section (only visible if the group has been drawn) -->
<?php if ($group->isDrawn() && $assignment): ?>
    <div class="row mb-4">
        <div class="col-12">
            <div class="card border-success">
                <div class="card-header bg-success text-white">
                    <h5 class="card-title mb-0"><?= t('group.show.assignment') ?></h5>
                </div>
                <div class="card-body">
                    <p class="card-text"><?= t('group.show.assignmentDescription') ?></p>
                    <!-- Display name of the person the current user needs to gift -->
                    <h3 class="text-center mb-4"><?= htmlspecialchars($assignment->getReceiver()->getName()) ?></h3>

                    <div class="d-grid gap-2 col-md-6 mx-auto">
                        <a href="/wishlist/view/<?= $assignment->getReceiverId() ?>/<?= $group->getId() ?>" class="btn btn-primary"><?= t('group.show.viewWishlist') ?></a>
                    </div>
                </div>
            </div>
        </div>
    </div>
<?php endif; ?>

<!-- Group members section -->
<div class="row mb-4">
    <div class="col-12">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h5 class="card-title mb-0"><?= t('group.show.members', ['count' => count($group->getMembers())]) ?></h5>
                <!-- Exclusions management (only available before draw) -->
                <?php if (!$group->isDrawn()): ?>
                    <a href="/exclusions/<?= $group->getId() ?>" class="btn btn-sm btn-secondary"><?= t('group.show.manageExclusions') ?></a>
                <?php endif; ?>
            </div>
            <div class="card-body">
                <?php if (empty($group->getMembers())): ?>
                    <p class="card-text"><?= t('group.show.noMembers') ?></p>
                <?php else: ?>
                    <!-- Display member cards in responsive grid -->
                    <div class="row row-cols-1 row-cols-md-2 row-cols-lg-3 g-4">
                        <?php foreach ($group->getMembers() as $member): ?>
                            <div class="col">
                                <div class="card h-100">
                                    <div class="card-body">
                                        <h5 class="card-title">
                                            <?= htmlspecialchars($member->getName()) ?>
                                            <!-- Show admin badge if member is group admin -->
                                            <?php if ($member->getId() === $group->getAdminId()): ?>
                                                <span class="badge bg-primary"><?= t('group.admin') ?></span>
                                            <?php endif; ?>
                                            <!-- Show 'You' badge if member is current user -->
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

<!-- User wishlist section -->
<div class="row mb-4">
    <div class="col-12">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h5 class="card-title mb-0"><?= t('group.show.yourWishlist') ?></h5>
                <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-sm btn-secondary"><?= t('group.show.editWishlist') ?></a>
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

<!-- JavaScript for copy to clipboard functionality -->
<script>
    /**
     * Copies the text content of an input element to the clipboard
     * 
     * @param {string} elementId - ID of the element containing text to copy
     */
    function copyToClipboard(elementId) {
        var input = document.getElementById(elementId);
        input.select();
        input.setSelectionRange(0, 99999);
        document.execCommand("copy");

        // Show toast or alert that copying was successful
        // alert("Copied to clipboard!");
    }
</script>