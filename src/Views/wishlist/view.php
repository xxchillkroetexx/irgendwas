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

<div class="card mb-4">
    <div class="card-body">
        <?php if ($wishlist && !empty($wishlist->getItems())): ?>
            <?php if ($wishlist->isPriorityOrdered()): ?>
                <div class="alert alert-info mb-4">
                    <i class="bi bi-info-circle-fill me-2"></i>
                    <?= t('wishlist.view.priorityOrdered') ?>
                </div>
            <?php endif; ?>

            <div class="row row-cols-1 row-cols-md-2 row-cols-lg-3 g-4">
                <?php foreach ($wishlist->getItems() as $item): ?>
                    <div class="col">
                        <div class="card h-100">
                            <div class="card-body">
                                <h5 class="card-title"><?= htmlspecialchars($item->getTitle()) ?></h5>

                                <?php if ($item->getDescription()): ?>
                                    <p class="card-text"><?= nl2br(htmlspecialchars($item->getDescription())) ?></p>
                                <?php endif; ?>

                                <?php if ($item->getLink()): ?>
                                    <a href="<?= htmlspecialchars($item->getLink()) ?>" class="btn btn-sm btn-outline-primary mt-2" target="_blank">
                                        <?= t('wishlist.view.viewItem') ?> <i class="bi bi-box-arrow-up-right ms-1"></i>
                                    </a>
                                <?php endif; ?>
                            </div>

                            <?php if ($wishlist->isPriorityOrdered()): ?>
                                <div class="card-footer text-muted">
                                    <small><?= t('wishlist.view.priority') ?>: <?= $item->getPosition() ?></small>
                                </div>
                            <?php endif; ?>
                        </div>
                    </div>
                <?php endforeach; ?>
            </div>
        <?php else: ?>
            <div class="text-center py-5">
                <h4 class="text-muted mb-4"><?= t('wishlist.view.empty') ?></h4>

                <?php if ($is_own_wishlist): ?>
                    <p><?= t('wishlist.edit.addSome') ?></p>
                    <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-primary"><?= t('wishlist.view.addNow') ?></a>
                <?php else: ?>
                    <p><?= t('wishlist.view.noPreference') ?></p>
                    <p><?= t('wishlist.view.suggestGiftCard') ?></p>
                <?php endif; ?>
            </div>
        <?php endif; ?>
    </div>
</div>

<div class="text-center">
    <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary">&larr; <?= t('wishlist.view.backToGroup') ?></a>
</div>