<div class="d-flex justify-content-between align-items-center mb-4">
    <h1><?= htmlspecialchars($user->getFullName()) ?>'s Wishlist</h1>
    <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary">Back to Group</a>
</div>

<div class="row">
    <!-- Left column: Wishlist -->
    <div class="col-lg-8">
        <div class="card mb-4">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h4 class="mb-0">Wishlist Items</h4>
                <?php if ($editable): ?>
                    <button class="btn btn-primary btn-sm" data-bs-toggle="modal" data-bs-target="#addItemModal">
                        <i class="fas fa-plus me-1"></i> Add Item
                    </button>
                <?php endif; ?>
            </div>
            <div class="card-body">
                <?php if (empty($wishlist->getItems())): ?>
                    <div class="alert alert-info">
                        <p class="mb-0">
                            <?php if ($editable): ?>
                                You haven't added any items to your wishlist yet. Click the "Add Item" button to get started.
                            <?php else: ?>
                                <?= htmlspecialchars($user->getFirstName()) ?> hasn't added any items to their wishlist yet.
                            <?php endif; ?>
                        </p>
                    </div>
                <?php else: ?>
                    <div id="wishlist-items" class="wishlist-items" data-group-id="<?= $group->getId() ?>">
                        <?php foreach ($wishlist->getItems() as $item): ?>
                            <div class="wishlist-item mb-3" data-item-id="<?= $item['id'] ?>">
                                <div class="d-flex justify-content-between">
                                    <div>
                                        <h5 class="mb-1"><?= htmlspecialchars($item['name']) ?></h5>
                                        
                                        <?php if ($item['description']): ?>
                                            <p class="mb-2"><?= nl2br(htmlspecialchars($item['description'])) ?></p>
                                        <?php endif; ?>
                                        
                                        <?php if ($item['link']): ?>
                                            <a href="<?= htmlspecialchars($item['link']) ?>" class="btn btn-sm btn-outline-primary" target="_blank">
                                                <i class="fas fa-external-link-alt me-1"></i> View Link
                                            </a>
                                        <?php endif; ?>
                                    </div>
                                    
                                    <?php if ($editable): ?>
                                        <div class="btn-group">
                                            <button class="btn btn-sm btn-outline-secondary wishlist-item-handle" title="Drag to reorder">
                                                <i class="fas fa-grip-lines"></i>
                                            </button>
                                            <button class="btn btn-sm btn-outline-secondary" 
                                                    data-bs-toggle="modal" 
                                                    data-bs-target="#editItemModal" 
                                                    data-item-id="<?= $item['id'] ?>"
                                                    data-item-name="<?= htmlspecialchars($item['name']) ?>"
                                                    data-item-description="<?= htmlspecialchars($item['description'] ?? '') ?>"
                                                    data-item-link="<?= htmlspecialchars($item['link'] ?? '') ?>">
                                                <i class="fas fa-edit"></i>
                                            </button>
                                            <form action="/groups/<?= $group->getId() ?>/wishlist/item/<?= $item['id'] ?>/delete" method="post" class="d-inline">
                                                <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                                                <button type="submit" class="btn btn-sm btn-outline-danger" onclick="return confirm('Are you sure you want to delete this item?')">
                                                    <i class="fas fa-trash-alt"></i>
                                                </button>
                                            </form>
                                        </div>
                                    <?php endif; ?>
                                </div>
                            </div>
                        <?php endforeach; ?>
                    </div>
                <?php endif; ?>
            </div>
        </div>
    </div>
    
    <!-- Right column: Group info -->
    <div class="col-lg-4">
        <div class="card mb-4">
            <div class="card-header">
                <h4 class="mb-0">Group: <?= htmlspecialchars($group->getName()) ?></h4>
            </div>
            <div class="card-body">
                <p class="mb-3">
                    <?php if ($editable): ?>
                        This is your wishlist for the group "<?= htmlspecialchars($group->getName()) ?>".
                    <?php else: ?>
                        You are viewing <?= htmlspecialchars($user->getFirstName()) ?>'s wishlist for the group "<?= htmlspecialchars($group->getName()) ?>".
                    <?php endif; ?>
                </p>
                
                <div class="d-grid">
                    <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-primary">
                        <i class="fas fa-arrow-left me-1"></i> Return to Group
                    </a>
                </div>
            </div>
        </div>
        
        <?php if ($editable): ?>
            <div class="card">
                <div class="card-header">
                    <h4 class="mb-0">Tips for Your Wishlist</h4>
                </div>
                <div class="card-body">
                    <ul class="list-unstyled mb-0">
                        <li class="mb-2">
                            <i class="fas fa-check-circle text-success me-2"></i>
                            Add a variety of items at different price points
                        </li>
                        <li class="mb-2">
                            <i class="fas fa-check-circle text-success me-2"></i>
                            Include specific details like size, color, etc.
                        </li>
                        <li class="mb-2">
                            <i class="fas fa-check-circle text-success me-2"></i>
                            Add links to make it easier for your Secret Santa
                        </li>
                        <li>
                            <i class="fas fa-check-circle text-success me-2"></i>
                            Drag items to reorder them by priority
                        </li>
                    </ul>
                </div>
            </div>
        <?php endif; ?>
    </div>
</div>

<?php if ($editable): ?>
<!-- Add Item Modal -->
<div class="modal fade" id="addItemModal" tabindex="-1" aria-labelledby="addItemModalLabel" aria-hidden="true">
    <div class="modal-dialog">
        <div class="modal-content">
            <form action="/groups/<?= $group->getId() ?>/wishlist/item" method="post">
                <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                
                <div class="modal-header">
                    <h5 class="modal-title" id="addItemModalLabel">Add Wishlist Item</h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <div class="mb-3">
                        <label for="name" class="form-label">Item Name <span class="text-danger">*</span></label>
                        <input type="text" class="form-control" id="name" name="name" required>
                    </div>
                    <div class="mb-3">
                        <label for="description" class="form-label">Description</label>
                        <textarea class="form-control" id="description" name="description" rows="3" placeholder="Optional details like size, color, etc."></textarea>
                    </div>
                    <div class="mb-3">
                        <label for="link" class="form-label">Link (optional)</label>
                        <input type="text" class="form-control" id="link" name="link" placeholder="https://...">
                        <div class="form-text">Add a link to where this item can be found</div>
                    </div>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">Cancel</button>
                    <button type="submit" class="btn btn-primary">Add Item</button>
                </div>
            </form>
        </div>
    </div>
</div>

<!-- Edit Item Modal -->
<div class="modal fade" id="editItemModal" tabindex="-1" aria-labelledby="editItemModalLabel" aria-hidden="true">
    <div class="modal-dialog">
        <div class="modal-content">
            <form id="editItemForm" action="" method="post">
                <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                
                <div class="modal-header">
                    <h5 class="modal-title" id="editItemModalLabel">Edit Wishlist Item</h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <div class="mb-3">
                        <label for="edit-name" class="form-label">Item Name <span class="text-danger">*</span></label>
                        <input type="text" class="form-control" id="edit-name" name="name" required>
                    </div>
                    <div class="mb-3">
                        <label for="edit-description" class="form-label">Description</label>
                        <textarea class="form-control" id="edit-description" name="description" rows="3"></textarea>
                    </div>
                    <div class="mb-3">
                        <label for="edit-link" class="form-label">Link (optional)</label>
                        <input type="text" class="form-control" id="edit-link" name="link">
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

<script>
document.addEventListener('DOMContentLoaded', function() {
    // Configure edit item modal
    const editItemModal = document.getElementById('editItemModal');
    if (editItemModal) {
        editItemModal.addEventListener('show.bs.modal', function (event) {
            const button = event.relatedTarget;
            const itemId = button.getAttribute('data-item-id');
            const itemName = button.getAttribute('data-item-name');
            const itemDescription = button.getAttribute('data-item-description');
            const itemLink = button.getAttribute('data-item-link');
            
            const form = editItemModal.querySelector('#editItemForm');
            form.action = `/groups/<?= $group->getId() ?>/wishlist/item/${itemId}`;
            
            form.querySelector('#edit-name').value = itemName;
            form.querySelector('#edit-description').value = itemDescription;
            form.querySelector('#edit-link').value = itemLink;
        });
    }
    
    // Initialize drag-and-drop for wishlist items if we have items and they're editable
    const wishlistContainer = document.getElementById('wishlist-items');
    if (wishlistContainer && <?= $editable ? 'true' : 'false' ?> && wishlistContainer.children.length > 0) {
        const sortable = new Sortable(wishlistContainer, {
            animation: 150,
            handle: '.wishlist-item-handle',
            ghostClass: 'wishlist-item-ghost',
            onEnd: function() {
                // Get the new order of items
                const itemOrder = Array.from(wishlistContainer.children).map(item => 
                    item.getAttribute('data-item-id')
                );
                
                // Send the new order to the server
                fetch(`/groups/<?= $group->getId() ?>/wishlist/reorder`, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'X-CSRF-Token': '<?= $csrf ?>'
                    },
                    body: JSON.stringify({ items: itemOrder })
                })
                .then(response => response.json())
                .then(data => {
                    if (!data.success) {
                        console.error('Failed to reorder items:', data.message);
                    }
                })
                .catch(error => {
                    console.error('Error reordering items:', error);
                });
            }
        });
    }
});
</script>
<?php endif; ?>