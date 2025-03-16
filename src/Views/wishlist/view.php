/**
 * Wishlist View Template
 * 
 * Displays a user's wishlist for a specific group.
 * Shows wishlist items with their details and handles various display states.
 * 
 * @var User $user The user whose wishlist is being viewed
 * @var Group $group The group context for this wishlist
 * @var Wishlist $wishlist The wishlist object containing items
 * @var bool $is_own_wishlist Whether the current user is viewing their own wishlist
 */

// Header section with title and group information
<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1><?= t('wishlist.view.title', ['name' => htmlspecialchars($user->getName())]) ?></h1>
        <p class="lead"><?= t('wishlist.view.forGroup') ?> <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a></p>
    </div>
    <div>
        <?php if ($is_own_wishlist): ?>
            <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-primary"><?= t('wishlist.edit.title') ?></a>
        <?php endif; ?>
    </div>
</div>

// Main wishlist content card
<div class="card mb-4">
    <div class="card-body">
        <?php if ($wishlist && !empty($wishlist->getItems())): ?>
            <?php if ($wishlist->isPriorityOrdered()): ?>
                <!-- Information alert for priority-ordered wishlists -->
                <div class="alert alert-info mb-4">
                    <i class="bi bi-info-circle-fill me-2"></i>
                    <?= t('wishlist.view.priorityOrdered') ?>
                </div>
            <?php endif; ?>

            <!-- Grid layout for wishlist items -->
            <div class="row row-cols-1 row-cols-md-2 row-cols-lg-3 g-4">
                <?php foreach ($wishlist->getItems() as $item): ?>
                    <!-- Individual item card -->
                    <div class="col">
                        <div class="card h-100">
                            <div class="card-body">
                                <!-- Item title -->
                                <h5 class="card-title"><?= htmlspecialchars($item->getTitle()) ?></h5>

                                <?php if ($item->getDescription()): ?>
                                    <!-- Item description, if available -->
                                    <p class="card-text"><?= nl2br(htmlspecialchars($item->getDescription())) ?></p>
                                <?php endif; ?>

                                <?php if ($item->getLink()): ?>
                                    <!-- External link to the item, if available -->
                                    <a href="<?= htmlspecialchars($item->getLink()) ?>" class="btn btn-sm btn-outline-primary mt-2" target="_blank">
                                        <?= t('wishlist.view.viewItem') ?> <i class="bi bi-box-arrow-up-right ms-1"></i>
                                    </a>
                                <?php endif; ?>
                            </div>

                            <?php if ($wishlist->isPriorityOrdered()): ?>
                                <!-- Priority indicator for ordered wishlists -->
                                <div class="card-footer text-muted">
                                    <small><?= t('wishlist.view.priority') ?>: <?= $item->getPosition() ?></small>
                                </div>
                            <?php endif; ?>
                        </div>
                    </div>
                <?php endforeach; ?>
            </div>
        <?php else: ?>
            <!-- Empty wishlist state -->
            <div class="text-center py-5">
                <h4 class="text-muted mb-4"><?= t('wishlist.view.empty') ?></h4>

                <?php if ($is_own_wishlist): ?>
                    <!-- Prompt for user to add items to their own empty wishlist -->
                    <p><?= t('wishlist.edit.addSome') ?></p>
                    <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-primary"><?= t('wishlist.view.addNow') ?></a>
                <?php else: ?>
                    <!-- Message shown when viewing someone else's empty wishlist -->
                    <p><?= t('wishlist.view.noPreference') ?></p>
                    <p><?= t('wishlist.view.suggestGiftCard') ?></p>
                <?php endif; ?>
            </div>
        <?php endif; ?>
    </div>
</div>

<!-- Navigation link to return to the group page -->
<div class="text-center">
    <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary">&larr; <?= t('wishlist.view.backToGroup') ?></a>
</div>