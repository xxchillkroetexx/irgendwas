<div class="d-flex justify-content-between align-items-center mb-4">
    <h1><?= htmlspecialchars($group->getName()) ?></h1>
    
    <div class="btn-group">
        <a href="/groups" class="btn btn-outline-secondary">Back to Groups</a>
        <?php if ($isAdmin): ?>
            <a href="/groups/<?= $group->getId() ?>/edit" class="btn btn-outline-primary">Edit Group</a>
        <?php endif; ?>
    </div>
</div>

<?php if ($group->getDescription()): ?>
    <div class="alert alert-light mb-4">
        <?= nl2br(htmlspecialchars($group->getDescription())) ?>
    </div>
<?php endif; ?>

<div class="row">
    <!-- Left column: Group info and wishlist -->
    <div class="col-lg-8">
        <?php if ($isAdmin && !$group->getIsDrawn()): ?>
            <div class="card mb-4 border-danger">
                <div class="card-header bg-danger text-white">
                    <h4 class="mb-0">Draw Secret Santas</h4>
                </div>
                <div class="card-body">
                    <p>Ready to assign Secret Santas? Once you perform the draw, participants will be notified by email.</p>
                    
                    <?php if (count($members) < 2): ?>
                        <div class="alert alert-warning">
                            You need at least 2 participants to perform the draw.
                        </div>
                    <?php else: ?>
                        <form action="/groups/<?= $group->getId() ?>/draw" method="post" 
                              onsubmit="return confirm('Are you sure you want to perform the draw? This will send emails to all participants.');">
                            <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                            <button type="submit" class="btn btn-danger">Perform Draw Now</button>
                        </form>
                    <?php endif; ?>
                </div>
            </div>
        <?php endif; ?>

        <?php if ($group->getIsDrawn()): ?>
            <div class="card mb-4 border-success">
                <div class="card-header bg-success text-white">
                    <h4 class="mb-0">Your Secret Santa Assignment</h4>
                </div>
                <div class="card-body">
                    <?php if ($assignment): ?>
                        <p>You are the Secret Santa for:</p>
                        <h3 class="mb-3"><?= htmlspecialchars($assignment->getFullName()) ?></h3>
                        <a href="/groups/<?= $group->getId() ?>/wishlist/<?= $assignment->getId() ?>" class="btn btn-primary">
                            <i class="fas fa-gift me-2"></i> View Their Wishlist
                        </a>
                        
                        <?php if ($isAdmin): ?>
                            <hr>
                            <div class="alert alert-warning">
                                <strong>Admin Note:</strong> As the group admin, you can see this information but keep it secret!
                            </div>
                        <?php endif; ?>
                    <?php else: ?>
                        <p>Something went wrong with your assignment. Please contact the group admin.</p>
                    <?php endif; ?>
                </div>
            </div>
        
            <?php if ($isAdmin): ?>
                <div class="card mb-4">
                    <div class="card-header">
                        <h4 class="mb-0">Admin Controls</h4>
                    </div>
                    <div class="card-body">
                        <form action="/groups/<?= $group->getId() ?>/redraw" method="post" class="mb-3"
                              onsubmit="return confirm('Are you sure you want to redo the draw? This will delete all current assignments and send new emails.');">
                            <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                            <button type="submit" class="btn btn-warning">Redo the Draw</button>
                            <small class="d-block text-muted mt-2">This will reset all current assignments and perform a new draw.</small>
                        </form>
                        
                        <h5 class="mt-4">Resend Assignment Email</h5>
                        <div class="list-group">
                            <?php foreach ($members as $member): ?>
                                <div class="list-group-item d-flex justify-content-between align-items-center">
                                    <?= htmlspecialchars($member->getFullName()) ?>
                                    <form action="/groups/<?= $group->getId() ?>/resend-email/<?= $member->getId() ?>" method="post">
                                        <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                                        <button type="submit" class="btn btn-sm btn-outline-secondary">Resend Email</button>
                                    </form>
                                </div>
                            <?php endforeach; ?>
                        </div>
                    </div>
                </div>
            <?php endif; ?>
        <?php endif; ?>

        <div class="card mb-4">
            <div class="card-header">
                <h4 class="mb-0">Your Wishlist</h4>
            </div>
            <div class="card-body">
                <?php if ($wishlist && !empty($wishlist->getItems())): ?>
                    <p>You've added <?= count($wishlist->getItems()) ?> items to your wishlist.</p>
                <?php else: ?>
                    <p>You haven't added any items to your wishlist yet.</p>
                <?php endif; ?>
                
                <a href="/groups/<?= $group->getId() ?>/wishlist" class="btn btn-primary">
                    <i class="fas fa-list-ul me-2"></i> Manage Your Wishlist
                </a>
            </div>
        </div>
    </div>
    
    <!-- Right column: Members and invitations -->
    <div class="col-lg-4">
        <div class="card mb-4">
            <div class="card-header d-flex justify-content-between align-items-center">
                <h4 class="mb-0">Members</h4>
                <span class="badge bg-primary"><?= count($members) ?></span>
            </div>
            <div class="card-body p-0">
                <ul class="list-group list-group-flush">
                    <?php foreach ($members as $member): ?>
                        <li class="list-group-item d-flex justify-content-between align-items-center">
                            <div>
                                <?= htmlspecialchars($member->getFullName()) ?>
                                <?php if ($group->isAdmin($member->getId())): ?>
                                    <span class="badge bg-info ms-2">Admin</span>
                                <?php endif; ?>
                            </div>
                            
                            <?php if ($group->getWishlistVisibility() === 'all' || $isAdmin): ?>
                                <a href="/groups/<?= $group->getId() ?>/wishlist/<?= $member->getId() ?>" class="btn btn-sm btn-outline-primary">
                                    <i class="fas fa-gift"></i>
                                </a>
                            <?php endif; ?>
                        </li>
                    <?php endforeach; ?>
                </ul>
            </div>
        </div>
        
        <?php if ($isAdmin): ?>
            <div class="card mb-4">
                <div class="card-header">
                    <h4 class="mb-0">Invite Members</h4>
                </div>
                <div class="card-body">
                    <form action="/groups/<?= $group->getId() ?>/invite" method="post">
                        <input type="hidden" name="csrf_token" value="<?= $csrf ?>">
                        
                        <div class="mb-3">
                            <label for="emails" class="form-label">Email Addresses</label>
                            <textarea class="form-control" id="emails" name="emails" rows="3" placeholder="Enter email addresses, separated by commas"></textarea>
                            <div class="form-text">Enter one or more email addresses, separated by commas</div>
                        </div>
                        
                        <button type="submit" class="btn btn-primary">Send Invitations</button>
                    </form>
                    
                    <?php if (!empty($invitations)): ?>
                        <hr>
                        <h5>Pending Invitations</h5>
                        <ul class="list-group">
                            <?php foreach ($invitations as $invitation): ?>
                                <li class="list-group-item d-flex justify-content-between align-items-center">
                                    <?= htmlspecialchars($invitation['invitation_email']) ?>
                                    <span class="badge bg-warning text-dark">Pending</span>
                                </li>
                            <?php endforeach; ?>
                        </ul>
                    <?php endif; ?>
                </div>
            </div>
        <?php endif; ?>
        
        <div class="card mb-4">
            <div class="card-header">
                <h4 class="mb-0">Group Details</h4>
            </div>
            <div class="card-body">
                <dl class="row mb-0">
                    <dt class="col-sm-5">Admin:</dt>
                    <dd class="col-sm-7"><?= htmlspecialchars($group->getAdmin()->getFullName()) ?></dd>
                    
                    <?php if ($group->getJoinDeadline()): ?>
                        <dt class="col-sm-5">Join Deadline:</dt>
                        <dd class="col-sm-7"><?= date('F j, Y', strtotime($group->getJoinDeadline())) ?></dd>
                    <?php endif; ?>
                    
                    <?php if ($group->getDrawDate()): ?>
                        <dt class="col-sm-5">Draw Date:</dt>
                        <dd class="col-sm-7"><?= date('F j, Y', strtotime($group->getDrawDate())) ?></dd>
                    <?php endif; ?>
                    
                    <dt class="col-sm-5">Created:</dt>
                    <dd class="col-sm-7"><?= date('F j, Y', strtotime($group->getCreatedAt())) ?></dd>
                    
                    <dt class="col-sm-5">Status:</dt>
                    <dd class="col-sm-7">
                        <?php if ($group->getIsDrawn()): ?>
                            <span class="badge bg-success">Drawn</span>
                        <?php else: ?>
                            <span class="badge bg-warning text-dark">Pending Draw</span>
                        <?php endif; ?>
                    </dd>
                    
                    <dt class="col-sm-5">Wishlist Visibility:</dt>
                    <dd class="col-sm-7">
                        <?= $group->getWishlistVisibility() === 'all' ? 'All members' : 'Santa only' ?>
                    </dd>
                </dl>
            </div>
        </div>
    </div>
</div>