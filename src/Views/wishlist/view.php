<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1><?= htmlspecialchars($user->getName()) ?>'s Wishlist</h1>
        <p class="lead">For group: <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a></p>
    </div>
    <div>
        <?php if ($is_own_wishlist): ?>
            <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-primary">Edit My Wishlist</a>
        <?php endif; ?>
    </div>
</div>

<div class="card mb-4">
    <div class="card-body">
        <?php if ($wishlist && !empty($wishlist->getItems())): ?>
            <?php if ($wishlist->isPriorityOrdered()): ?>
                <div class="alert alert-info mb-4">
                    <i class="bi bi-info-circle-fill me-2"></i>
                    This wishlist is priority ordered. Items at the top are higher priority.
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
                                        View Item <i class="bi bi-box-arrow-up-right ms-1"></i>
                                    </a>
                                <?php endif; ?>
                            </div>

                            <?php if ($wishlist->isPriorityOrdered()): ?>
                                <div class="card-footer text-muted">
                                    <small>Priority: <?= $item->getPosition() ?></small>
                                </div>
                            <?php endif; ?>
                        </div>
                    </div>
                <?php endforeach; ?>
            </div>
        <?php else: ?>
            <div class="text-center py-5">
                <h4 class="text-muted mb-4">No items in this wishlist yet</h4>

                <?php if ($is_own_wishlist): ?>
                    <p>Add some items to help your Secret Santa know what you'd like!</p>
                    <a href="/wishlist/edit/<?= $group->getId() ?>" class="btn btn-primary">Add Items Now</a>
                <?php else: ?>
                    <p>This person hasn't added any items to their wishlist yet.</p>
                    <p>Check back later or consider a gift card!</p>
                <?php endif; ?>
            </div>
        <?php endif; ?>
    </div>
</div>

<div class="text-center">
    <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary">&larr; Back to Group</a>
</div>