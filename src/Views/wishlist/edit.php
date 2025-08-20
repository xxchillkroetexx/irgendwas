<?php

/**
 * Wishlist Edit Template
 * 
 * Provides functionality to add, edit, remove and prioritize wishlist items.
 * Includes settings to control wishlist behavior.
 * 
 * @var Group $group The group context for this wishlist
 * @var Wishlist $wishlist The wishlist being edited
 * @var Auth $auth Authentication service to get current user ID
 * @var array $errors Validation errors for form fields
 * @var array $old Previously submitted form values
 */
?>

<!-- Header section with title and navigation -->
<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1><?= t('wishlist.edit.title') ?></h1>
        <p class="lead"><?= t('wishlist.edit.forGroup') ?> <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a></p>
    </div>
    <div>
        <a href="/wishlist/view/<?= $auth->userId() ?>/<?= $group->getId() ?>" class="btn btn-outline-secondary"><?= t('wishlist.view.title', ['name' => t('group.show.you')]) ?></a>
    </div>
</div>

<!-- Wishlist Settings Section -->
<div class="card mb-4">
    <div class="card-header">
        <h5 class="card-title mb-0">Wishlist Settings</h5>
    </div>
    <div class="card-body">
        <form action="/wishlist/<?= $group->getId() ?>/settings" method="post">
            <!-- Priority ordering toggle switch -->
            <div class="form-check form-switch mb-3">
                <input class="form-check-input" type="checkbox" id="isPriorityOrdered" name="is_priority_ordered" value="1" <?= $wishlist->isPriorityOrdered() ? 'checked' : '' ?>>
                <label class="form-check-label" for="isPriorityOrdered">
                    Priority Order Items (drag and reorder items to indicate preference)
                </label>
            </div>
            <button type="submit" class="btn btn-primary">Save Settings</button>
        </form>
    </div>
</div>

<!-- Add New Item Form -->
<div class="card mb-4">
    <div class="card-header">
        <h5 class="card-title mb-0"><?= t('wishlist.edit.addItem') ?></h5>
    </div>
    <div class="card-body">
        <form action="/wishlist/<?= $group->getId() ?>/item/add" method="post">
            <!-- Title field - required -->
            <div class="mb-3">
                <label for="title" class="form-label"><?= t('wishlist.edit.itemTitle') ?> *</label>
                <input type="text" class="form-control" id="title" name="title" required
                    value="<?= isset($old['title']) ? htmlspecialchars($old['title']) : '' ?>">
                <?php if (isset($errors['title'])): ?>
                    <div class="text-danger"><?= htmlspecialchars($errors['title']) ?></div>
                <?php endif; ?>
            </div>

            <!-- Description field - optional -->
            <div class="mb-3">
                <label for="description" class="form-label"><?= t('wishlist.edit.itemDescription') ?></label>
                <textarea class="form-control" id="description" name="description" rows="3"><?= isset($old['description']) ? htmlspecialchars($old['description']) : '' ?></textarea>
                <div class="form-text"><?= t('wishlist.edit.descriptionHelp') ?></div>
            </div>

            <!-- Link field - optional URL -->
            <div class="mb-3">
                <label for="link" class="form-label"><?= t('wishlist.edit.link') ?></label>
                <input type="url" class="form-control" id="link" name="link"
                    value="<?= isset($old['link']) ? htmlspecialchars($old['link']) : '' ?>">
                <div class="form-text"><?= t('wishlist.edit.linkHelp') ?></div>
                <?php if (isset($errors['link'])): ?>
                    <div class="text-danger"><?= htmlspecialchars($errors['link']) ?></div>
                <?php endif; ?>
            </div>

            <button type="submit" class="btn btn-success"><?= t('wishlist.edit.addItem') ?></button>
        </form>
    </div>
</div>

<!-- Existing Items Table -->
<div class="card">
    <div class="card-header d-flex justify-content-between align-items-center">
        <h5 class="card-title mb-0"><?= t('wishlist.edit.title') ?></h5>
        <?php if ($wishlist->isPriorityOrdered() && !empty($wishlist->getItems())): ?>
            <button type="button" class="btn btn-outline-primary btn-sm" id="editPriorityBtn">Edit Priority Order</button>
        <?php endif; ?>
    </div>
    <div class="card-body">
        <?php if (empty($wishlist->getItems())): ?>
            <!-- Empty state when no items exist -->
            <p class="text-center py-4"><?= t('wishlist.edit.noItems') ?></p>
        <?php else: ?>
            <!-- Table of existing wishlist items -->
            <div class="table-responsive">
                <table class="table table-hover">
                    <thead>
                        <tr>
                            <?php if ($wishlist->isPriorityOrdered()): ?>
                                <th width="5%">Priority</th>
                            <?php endif; ?>
                            <th>Item</th>
                            <th width="15%">Actions</th>
                        </tr>
                    </thead>
                    <tbody id="wishlistItems">
                        <?php foreach ($wishlist->getItems() as $item): ?>
                            <tr data-item-id="<?= $item->getId() ?>">
                                <?php if ($wishlist->isPriorityOrdered()): ?>
                                    <!-- Priority number and hidden input for ordering -->
                                    <td class="text-center priority-handle" style="cursor: grab;">
                                        <span class="badge bg-secondary"><?= $item->getPosition() ?></span>
                                        <input type="hidden" name="positions[<?= $item->getId() ?>]" value="<?= $item->getPosition() ?>">
                                    </td>
                                <?php endif; ?>
                                <td>
                                    <!-- Item details -->
                                    <div class="fw-bold"><?= htmlspecialchars($item->getTitle()) ?></div>
                                    <?php if ($item->getDescription()): ?>
                                        <div class="text-muted small"><?= nl2br(htmlspecialchars($item->getDescription())) ?></div>
                                    <?php endif; ?>
                                    <?php if ($item->getLink()): ?>
                                        <div class="mt-1">
                                            <a href="<?= htmlspecialchars($item->getLink()) ?>" target="_blank" class="small">
                                                <?= t('wishlist.view.viewItem') ?> <i class="bi bi-box-arrow-up-right"></i>
                                            </a>
                                        </div>
                                    <?php endif; ?>
                                </td>
                                <td>
                                    <!-- Item action buttons -->
                                    <div class="btn-group btn-group-sm">
                                        <button type="button" class="btn btn-outline-primary edit-item-btn"
                                            data-id="<?= $item->getId() ?>"
                                            data-title="<?= htmlspecialchars($item->getTitle()) ?>"
                                            data-description="<?= htmlspecialchars($item->getDescription() ?? '') ?>"
                                            data-link="<?= htmlspecialchars($item->getLink() ?? '') ?>">
                                            <?= t('wishlist.edit.edit') ?>
                                        </button>
                                        <a href="/wishlist/item/<?= $item->getId() ?>/delete" class="btn btn-outline-danger"
                                            onclick="return confirm('<?= t('wishlist.edit.removeItemConfirm') ?>')">
                                            <?= t('wishlist.edit.removeItem') ?>
                                        </a>
                                    </div>
                                </td>
                            </tr>
                        <?php endforeach; ?>
                    </tbody>
                </table>
            </div>
        <?php endif; ?>
    </div>
</div>

<!-- Danger Zone - Delete Wishlist -->
<div class="card border-danger">
    <div class="card-header bg-danger text-white">
        <h5 class="card-title mb-0">
            <i class="bi bi-exclamation-triangle-fill me-2"></i>
            Danger Zone
        </h5>
    </div>
    <div class="card-body">
        <h6 class="card-subtitle mb-2 text-muted"><?= t('wishlist.edit.deleteWishlist') ?></h6>
        <p class="text-muted">
            This will permanently delete your entire wishlist and all items. This action cannot be undone.
        </p>
        <a href="/wishlist/<?= $group->getId() ?>/delete"
            class="btn btn-danger"
            onclick="return confirm('<?= t('wishlist.edit.deleteWishlistConfirm') ?>')">
            <i class="bi bi-trash3-fill me-2"></i>
            <?= t('wishlist.edit.deleteWishlist') ?>
        </a>
    </div>
</div>

<!-- Edit Item Modal -->
<div class="modal fade" id="editItemModal" tabindex="-1" aria-labelledby="editItemModalLabel" aria-hidden="true">
    <div class="modal-dialog">
        <div class="modal-content">
            <form id="editItemForm" action="" method="post">
                <div class="modal-header">
                    <h5 class="modal-title" id="editItemModalLabel"><?= t('wishlist.edit.edit') ?> <?= t('wishlist.edit.itemTitle') ?></h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <!-- Edit item form fields -->
                    <div class="mb-3">
                        <label for="editTitle" class="form-label"><?= t('wishlist.edit.itemTitle') ?> *</label>
                        <input type="text" class="form-control" id="editTitle" name="title" required>
                    </div>

                    <div class="mb-3">
                        <label for="editDescription" class="form-label"><?= t('wishlist.edit.itemDescription') ?></label>
                        <textarea class="form-control" id="editDescription" name="description" rows="3"></textarea>
                        <div class="form-text"><?= t('wishlist.edit.descriptionHelp') ?></div>
                    </div>

                    <div class="mb-3">
                        <label for="editLink" class="form-label"><?= t('wishlist.edit.link') ?></label>
                        <input type="url" class="form-control" id="editLink" name="link">
                        <div class="form-text"><?= t('wishlist.edit.linkHelp') ?></div>
                    </div>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary" data-bs-dismiss="modal"><?= t('wishlist.edit.cancel') ?></button>
                    <button type="submit" class="btn btn-primary"><?= t('wishlist.edit.save') ?></button>
                </div>
            </form>
        </div>
    </div>
</div>

<!-- Priority Order Modal -->
<div class="modal fade" id="priorityOrderModal" tabindex="-1" aria-labelledby="priorityOrderModalLabel" aria-hidden="true">
    <div class="modal-dialog">
        <div class="modal-content">
            <form action="/wishlist/<?= $group->getId() ?>/priority" method="post">
                <div class="modal-header">
                    <h5 class="modal-title" id="priorityOrderModalLabel">Edit Priority Order</h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <p class="text-muted">Drag and drop items to set your priority order. Items at the top have higher priority.</p>

                    <!-- Sortable list of wishlist items -->
                    <ul class="list-group" id="sortableItems">
                        <?php foreach ($wishlist->getItems() as $item): ?>
                            <li class="list-group-item d-flex justify-content-between align-items-center" data-item-id="<?= $item->getId() ?>">
                                <span class="priority-handle me-2" style="cursor: grab;">
                                    <i class="bi bi-grip-vertical"></i>
                                </span>
                                <div class="flex-grow-1">
                                    <?= htmlspecialchars($item->getTitle()) ?>
                                </div>
                                <input type="hidden" name="positions[<?= $item->getId() ?>]" value="<?= $item->getPosition() ?>" class="position-input">
                                <span class="badge bg-secondary position-display"><?= $item->getPosition() ?></span>
                            </li>
                        <?php endforeach; ?>
                    </ul>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">Cancel</button>
                    <button type="submit" class="btn btn-primary">Save Order</button>
                </div>
            </form>
        </div>
    </div>
</div>

<!-- Client-side JavaScript -->
<script>
    document.addEventListener('DOMContentLoaded', function() {
        // Edit item modal functionality
        const editButtons = document.querySelectorAll('.edit-item-btn');
        const editForm = document.getElementById('editItemForm');
        const editTitle = document.getElementById('editTitle');
        const editDescription = document.getElementById('editDescription');
        const editLink = document.getElementById('editLink');

        // Attach event listeners to all edit buttons
        editButtons.forEach(button => {
            button.addEventListener('click', function() {
                // Get item data from data attributes
                const id = this.dataset.id;
                const title = this.dataset.title;
                const description = this.dataset.description;
                const link = this.dataset.link;

                // Populate the edit form with item data
                editForm.action = `/wishlist/item/${id}/update`;
                editTitle.value = title;
                editDescription.value = description;
                editLink.value = link;

                // Show the modal
                const modal = new bootstrap.Modal(document.getElementById('editItemModal'));
                modal.show();
            });
        });

        // Priority order modal event handling
        const editPriorityBtn = document.getElementById('editPriorityBtn');
        if (editPriorityBtn) {
            editPriorityBtn.addEventListener('click', function() {
                const modal = new bootstrap.Modal(document.getElementById('priorityOrderModal'));
                modal.show();
            });
        }

        // TODO: Implement drag and drop sorting functionality
        // This would require a JavaScript library like Sortable.js
        // For now, this is a placeholder for future enhancement
    });
</script>