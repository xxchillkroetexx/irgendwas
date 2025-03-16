<?php
/**
 * User Dashboard View
 *
 * This view displays the main user dashboard with group membership information,
 * gift assignments, and quick access to common actions.
 *
 * @package     GiftExchange
 * @subpackage  Views
 * @category    User
 * 
 * @param User $user Current logged-in user object
 * @param Group[] $groups Array of Group objects the user is a member of
 * @param Assignment[] $assignments Array of gift assignments for the user
 * 
 * @author      GiftExchange Team
 * @version     1.0
 */
?>
<div class="row">
    <div class="col-md-8">
        <!-- User welcome section with personalized greeting -->
        <h2><?= t('user.dashboard.welcome', ['name' => htmlspecialchars($user->getName())]) ?></h2>
        <p class="text-muted"><?= t('user.dashboard.subtitle') ?></p>

        <div class="row mt-4">
            <div class="col-md-6">
                <!-- 
                 * Groups card component
                 * Displays all groups the user participates in with status indicators
                 * Shows different states for drawn and pending groups
                 -->
                <div class="card mb-4">
                    <div class="card-header bg-primary text-white">
                        <h5 class="mb-0"><?= t('user.dashboard.groups.title') ?></h5>
                    </div>
                    <div class="card-body">
                        <?php if (empty($groups)): ?>
                            <!-- Empty state when user has no groups -->
                            <p><?= t('user.dashboard.groups.empty') ?></p>
                            <a href="/groups/create" class="btn btn-primary"><?= t('user.dashboard.groups.createButton') ?></a>
                        <?php else: ?>
                            <!-- Display list of groups the user belongs to -->
                            <ul class="list-group">
                                <?php foreach ($groups as $group): ?>
                                    <li class="list-group-item d-flex justify-content-between align-items-center">
                                        <div>
                                            <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a>
                                            <?php if ($group->isDrawn()): ?>
                                                <!-- Indicator for groups where assignments have been made -->
                                                <span class="badge bg-success rounded-pill ms-2"><?= t('user.dashboard.groups.drawn') ?></span>
                                            <?php else: ?>
                                                <!-- Indicator for groups waiting for drawing -->
                                                <span class="badge bg-warning rounded-pill ms-2"><?= t('user.dashboard.groups.pending') ?></span>
                                            <?php endif; ?>
                                        </div>
                                        <?php if ($group->getAdminId() === $user->getId()): ?>
                                            <!-- Admin badge for groups the user administrates -->
                                            <span class="badge bg-secondary"><?= t('user.dashboard.groups.admin') ?></span>
                                        <?php endif; ?>
                                    </li>
                                <?php endforeach; ?>
                            </ul>
                            <!-- Group action buttons -->
                            <div class="mt-3">
                                <a href="/groups/create" class="btn btn-primary"><?= t('user.dashboard.groups.createButton') ?></a>
                                <a href="/groups" class="btn btn-outline-primary"><?= t('user.dashboard.groups.viewAll') ?></a>
                            </div>
                        <?php endif; ?>
                    </div>
                </div>
            </div>

            <div class="col-md-6">
                <!-- 
                 * Assignments card component
                 * Shows all gift recipient assignments for the current user
                 * Provides direct links to recipient wishlists
                 -->
                <div class="card mb-4">
                    <div class="card-header bg-success text-white">
                        <h5 class="mb-0"><?= t('user.dashboard.assignments.title') ?></h5>
                    </div>
                    <div class="card-body">
                        <?php if (empty($assignments)): ?>
                            <p><?= t('user.dashboard.assignments.empty') ?></p>
                            <p class="text-muted"><?= t('user.dashboard.assignments.emptyDescription') ?></p>
                        <?php else: ?>
                            <ul class="list-group">
                                <?php foreach ($assignments as $assignment): ?>
                                    <li class="list-group-item">
                                        <strong><?= t('user.dashboard.assignments.group') ?>:</strong> <?= htmlspecialchars($assignment->getGroup()->getName()) ?><br>
                                        <strong><?= t('user.dashboard.assignments.recipient') ?>:</strong> <?= htmlspecialchars($assignment->getReceiver()->getName()) ?><br>
                                        <a href="/wishlist/view/<?= $assignment->getReceiverId() ?>/<?= $assignment->getGroupId() ?>" class="btn btn-sm btn-outline-success mt-2">
                                            <?= t('user.dashboard.assignments.viewWishlist') ?>
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
        <!-- 
         * Quick Actions panel
         * Provides shortcut buttons for common user operations
         * Improves navigation efficiency for frequent tasks
         -->
        <div class="card mb-4">
            <div class="card-header bg-info text-white">
                <h5 class="mb-0"><?= t('user.dashboard.quickActions.title') ?></h5>
            </div>
            <div class="card-body">
                <div class="d-grid gap-2">
                    <a href="/groups/create" class="btn btn-outline-primary"><?= t('user.dashboard.quickActions.createGroup') ?></a>
                    <a href="/user/profile" class="btn btn-outline-secondary"><?= t('user.dashboard.quickActions.editProfile') ?></a>
                    <a href="/groups" class="btn btn-outline-info"><?= t('user.dashboard.quickActions.manageGroups') ?></a>
                </div>
            </div>
        </div>

        <!-- 
         * Tips and best practices panel
         * Provides guidance for optimal user experience
         * Helps users understand key features of the platform
         -->
        <div class="card">
            <div class="card-header bg-light">
                <h5 class="mb-0"><?= t('user.dashboard.tips.title') ?></h5>
            </div>
            <div class="card-body">
                <ul class="list-unstyled">
                    <li class="mb-2">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check-circle-fill text-success me-2" viewBox="0 0 16 16">
                            <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z" />
                        </svg>
                        <?= t('user.dashboard.tips.createWishlist') ?>
                    </li>
                    <li class="mb-2">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check-circle-fill text-success me-2" viewBox="0 0 16 16">
                            <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z" />
                        </svg>
                        <?= t('user.dashboard.tips.inviteFriends') ?>
                    </li>
                    <li class="mb-2">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-check-circle-fill text-success me-2" viewBox="0 0 16 16">
                            <path d="M16 8A8 8 0 1 1 0 8a8 8 0 0 1 16 0zm-3.97-3.03a.75.75 0 0 0-1.08.022L7.477 9.417 5.384 7.323a.75.75 0 0 0-1.06 1.06L6.97 11.03a.75.75 0 0 0 1.079-.02l3.992-4.99a.75.75 0 0 0-.01-1.05z" />
                        </svg>
                        <?= t('user.dashboard.tips.setExclusions') ?>
                    </li>
                </ul>
            </div>
        </div>
    </div>
</div>