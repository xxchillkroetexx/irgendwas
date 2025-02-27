<div class="row">
    <div class="col-lg-8 mx-auto">
        <div class="card">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h4 class="mb-0">Edit Group</h4>
                <a href="/groups/<?= $group->getId() ?>" class="btn btn-outline-secondary btn-sm">Back to Group</a>
            </div>
            <div class="card-body">
                <form action="/groups/<?= $group->getId() ?>" method="post">
                    <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                    
                    <div class="mb-3">
                        <label for="name" class="form-label">Group Name <span class="text-danger">*</span></label>
                        <input type="text" class="form-control <?= isset($errors['name']) ? 'is-invalid' : '' ?>" 
                            id="name" name="name" value="<?= $name ?? $group->getName() ?>" required>
                        <?php if (isset($errors['name'])): ?>
                            <div class="invalid-feedback"><?= $errors['name'] ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <div class="mb-3">
                        <label for="description" class="form-label">Description</label>
                        <textarea class="form-control <?= isset($errors['description']) ? 'is-invalid' : '' ?>" 
                            id="description" name="description" rows="3"><?= $description ?? $group->getDescription() ?></textarea>
                        <?php if (isset($errors['description'])): ?>
                            <div class="invalid-feedback"><?= $errors['description'] ?></div>
                        <?php endif; ?>
                    </div>
                    
                    <div class="row">
                        <div class="col-md-6">
                            <div class="mb-3">
                                <label for="join_deadline" class="form-label">Join Deadline</label>
                                <input type="date" class="form-control <?= isset($errors['join_deadline']) ? 'is-invalid' : '' ?>" 
                                    id="join_deadline" name="join_deadline" 
                                    value="<?= isset($join_deadline) ? $join_deadline : ($group->getJoinDeadline() ? date('Y-m-d', strtotime($group->getJoinDeadline())) : '') ?>">
                                <?php if (isset($errors['join_deadline'])): ?>
                                    <div class="invalid-feedback"><?= $errors['join_deadline'] ?></div>
                                <?php endif; ?>
                            </div>
                        </div>
                        <div class="col-md-6">
                            <div class="mb-3">
                                <label for="draw_date" class="form-label">Draw Date</label>
                                <input type="date" class="form-control <?= isset($errors['draw_date']) ? 'is-invalid' : '' ?>" 
                                    id="draw_date" name="draw_date" 
                                    value="<?= isset($draw_date) ? $draw_date : ($group->getDrawDate() ? date('Y-m-d', strtotime($group->getDrawDate())) : '') ?>">
                                <?php if (isset($errors['draw_date'])): ?>
                                    <div class="invalid-feedback"><?= $errors['draw_date'] ?></div>
                                <?php endif; ?>
                            </div>
                        </div>
                    </div>
                    
                    <div class="mb-3">
                        <label class="form-label">Wishlist Visibility</label>
                        <div class="form-check">
                            <input class="form-check-input" type="radio" name="wishlist_visibility" id="visibility_all" 
                                value="all" <?= isset($wishlist_visibility) ? ($wishlist_visibility == 'all' ? 'checked' : '') : ($group->getWishlistVisibility() == 'all' ? 'checked' : '') ?>>
                            <label class="form-check-label" for="visibility_all">
                                All group members can see each other's wishlists
                            </label>
                        </div>
                        <div class="form-check">
                            <input class="form-check-input" type="radio" name="wishlist_visibility" id="visibility_santa" 
                                value="santa_only" <?= isset($wishlist_visibility) ? ($wishlist_visibility == 'santa_only' ? 'checked' : '') : ($group->getWishlistVisibility() == 'santa_only' ? 'checked' : '') ?>>
                            <label class="form-check-label" for="visibility_santa">
                                Only Secret Santas can see their recipient's wishlist
                            </label>
                        </div>
                    </div>
                    
                    <div class="mb-4">
                        <label for="custom_email_template" class="form-label">Custom Email Template</label>
                        <textarea class="form-control" id="custom_email_template" name="custom_email_template" rows="5"><?= $custom_email_template ?? $group->getCustomEmailTemplate() ?></textarea>
                        <div class="form-text">
                            You can customize the email sent to participants when the draw is performed. 
                            Use {{recipient_name}} to insert the recipient's name and {{group_name}} for the group name.
                            Leave blank to use the default template.
                        </div>
                    </div>
                    
                    <div class="d-flex justify-content-between">
                        <button type="submit" class="btn btn-primary">Save Changes</button>
                        
                        <button type="button" class="btn btn-danger" data-bs-toggle="modal" data-bs-target="#deleteGroupModal">
                            Delete Group
                        </button>
                    </div>
                </form>
            </div>
        </div>
    </div>
</div>

<!-- Delete Group Modal -->
<div class="modal fade" id="deleteGroupModal" tabindex="-1" aria-labelledby="deleteGroupModalLabel" aria-hidden="true">
    <div class="modal-dialog">
        <div class="modal-content">
            <div class="modal-header">
                <h5 class="modal-title" id="deleteGroupModalLabel">Confirm Deletion</h5>
                <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
            </div>
            <div class="modal-body">
                <p>Are you sure you want to delete the group "<strong><?= htmlspecialchars($group->getName()) ?></strong>"?</p>
                <p class="text-danger">This action cannot be undone. All members, wishlists, and assignments will be permanently removed.</p>
            </div>
            <div class="modal-footer">
                <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">Cancel</button>
                <form action="/groups/<?= $group->getId() ?>/delete" method="post">
                    <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                    <button type="submit" class="btn btn-danger">Delete Group</button>
                </form>
            </div>
        </div>
    </div>
</div>
