<div class="d-flex justify-content-between align-items-start mb-4">
    <div>
        <h1>Edit Your Wishlist</h1>
        <p class="lead">For group: <a href="/groups/<?= $group->getId() ?>"><?= htmlspecialchars($group->getName()) ?></a></p>
    </div>
    <div>
        <a href="/wishlist/view/<?= $auth->userId() ?>/<?= $group->getId() ?>" class="btn btn-outline-secondary">View My Wishlist</a>
    </div>
</div>

<!-- Wishlist Settings -->
<div class="card mb-4">
    <div class="card-header">
        <h5 class="card-title mb-0">Wishlist Settings</h5>
    </div>
    <div class="card-body">
        <form action="/wishlist/<?= $group->getId() ?>/settings" method="post">
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
        <h5 class="card-title mb-0">Add New Item</h5>
    </div>
    <div class="card-body">
        <form action="/wishlist/<?= $group->getId() ?>/item/add" method="post">
            <div class="mb-3">
                <label for="title" class="form-label">Item Title *</label>
                <input type="text" class="form-control" id="title" name="title" required
                    value="<?= isset($old['title']) ? htmlspecialchars($old['title']) : '' ?>">
                <?php if (isset($errors['title'])): ?>
                    <div class="text-danger"><?= htmlspecialchars($errors['title']) ?></div>
                <?php endif; ?>
            </div>

            <div class="mb-3">
                <label for="description" class="form-label">Description</label>
                <textarea class="form-control" id="description" name="description" rows="3"><?= isset($old['description']) ? htmlspecialchars($old['description']) : '' ?></textarea>
                <div class="form-text">Optional. Add details like size, color, or model.</div>
            </div>

            <div class="mb-3">
                <label for="link" class="form-label">Link</label>
                <input type="url" class="form-control" id="link" name="link"
                    value="<?= isset($old['link']) ? htmlspecialchars($old['link']) : '' ?>">
                <div class="form-text">Optional. Add a link to the item online.</div>
                <?php if (isset($errors['link'])): ?>
                    <div class="text-danger"><?= htmlspecialchars($errors['link']) ?></div>
                <?php endif; ?>
            </div>

            <button type="submit" class="btn btn-success">Add Item</button>
        </form>
    </div>
</div>

<!-- Existing Items -->
<div class="card">
    <div class="card-header d-flex justify-content-between align-items-center">
        <h5 class="card-title mb-0">My Wishlist Items</h5>
        <?php if ($wishlist->isPriorityOrdered() && !empty($wishlist->getItems())): ?>
            <button type="button" class="btn btn-outline-primary btn-sm" id="editPriorityBtn">Edit Priority Order</button>
        <?php endif; ?>
    </div>
    <div class="card-body">
        <?php if (empty($wishlist->getItems())): ?>
            <p class="text-center py-4">You haven't added any items to your wishlist yet.</p>
        <?php else: ?>
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
                                    <td class="text-center priority-handle" style="cursor: grab;">
                                        <span class="badge bg-secondary"><?= $item->getPosition() ?></span>
                                        <input type="hidden" name="positions[<?= $item->getId() ?>]" value="<?= $item->getPosition() ?>">
                                    </td>
                                <?php endif; ?>
                                <td>
                                    <div class="fw-bold"><?= htmlspecialchars($item->getTitle()) ?></div>
                                    <?php if ($item->getDescription()): ?>
                                        <div class="text-muted small"><?= nl2br(htmlspecialchars($item->getDescription())) ?></div>
                                    <?php endif; ?>
                                    <?php if ($item->getLink()): ?>
                                        <div class="mt-1">
                                            <a href="<?= htmlspecialchars($item->getLink()) ?>" target="_blank" class="small">
                                                View Item <i class="bi bi-box-arrow-up-right"></i>
                                            </a>
                                        </div>
                                    <?php endif; ?>
                                </td>
                                <td>
                                    <div class="btn-group btn-group-sm">
                                        <button type="button" class="btn btn-outline-primary edit-item-btn"
                                            data-id="<?= $item->getId() ?>"
                                            data-title="<?= htmlspecialchars($item->getTitle()) ?>"
                                            data-description="<?= htmlspecialchars($item->getDescription() ?? '') ?>"
                                            data-link="<?= htmlspecialchars($item->getLink() ?? '') ?>">
                                            Edit
                                        </button>
                                        <a href="/wishlist/item/<?= $item->getId() ?>/delete" class="btn btn-outline-danger"
                                            onclick="return confirm('Are you sure you want to delete this item?')">
                                            Delete
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

<!-- Edit Item Modal -->
<div class="modal fade" id="editItemModal" tabindex="-1" aria-labelledby="editItemModalLabel" aria-hidden="true">
    <div class="modal-dialog">
        <div class="modal-content">
            <form id="editItemForm" action="" method="post">
                <div class="modal-header">
                    <h5 class="modal-title" id="editItemModalLabel">Edit Item</h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <div class="mb-3">
                        <label for="editTitle" class="form-label">Item Title *</label>
                        <input type="text" class="form-control" id="editTitle" name="title" required>
                    </div>

                    <div class="mb-3">
                        <label for="editDescription" class="form-label">Description</label>
                        <textarea class="form-control" id="editDescription" name="description" rows="3"></textarea>
                        <div class="form-text">Optional. Add details like size, color, or model.</div>
                    </div>

                    <div class="mb-3">
                        <label for="editLink" class="form-label">Link</label>
                        <input type="url" class="form-control" id="editLink" name="link">
                        <div class="form-text">Optional. Add a link to the item online.</div>
                    </div>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">Cancel</button>
                    <button type="submit" class="btn btn-primary">Save Changes</button>
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

<script>
    document.addEventListener('DOMContentLoaded', function() {
        // Edit item modal functionality
        const editButtons = document.querySelectorAll('.edit-item-btn');
        const editForm = document.getElementById('editItemForm');
        const editTitle = document.getElementById('editTitle');
        const editDescription = document.getElementById('editDescription');
        const editLink = document.getElementById('editLink');

        editButtons.forEach(button => {
            button.addEventListener('click', function() {
                const id = this.dataset.id;
                const title = this.dataset.title;
                const description = this.dataset.description;
                const link = this.dataset.link;

                editForm.action = `/wishlist/item/${id}/update`;
                editTitle.value = title;
                editDescription.value = description;
                editLink.value = link;

                const modal = new bootstrap.Modal(document.getElementById('editItemModal'));
                modal.show();
            });
        });

        // Priority order modal
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